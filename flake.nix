{
  description = "nativelink";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    nix2container = {
      url = "github:nlewo/nix2container";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.flake-utils.follows = "flake-utils";
    };
  };

  outputs = inputs @ {
    self,
    flake-parts,
    crane,
    rust-overlay,
    nix2container,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = [
        "x86_64-linux"
      ];
      imports = [
        inputs.git-hooks.flakeModule
      ];
      perSystem = {
        config,
        pkgs,
        system,
        ...
      }: let
        stable-rust-version = "1.78.0";
        nightly-rust-version = "2024-05-10";

        stable-rust = pkgs.pkgsMusl.rust-bin.stable.${stable-rust-version};
        nightly-rust = pkgs.pkgsMusl.rust-bin.nightly.${nightly-rust-version};

        # TODO(aaronmondal): Tools like rustdoc don't work with the `pkgsMusl`
        # package set because of missing libgcc_s. Fix this upstream and use the
        # `stable-rust` toolchain in the devShell as well.
        # See: https://github.com/oxalica/rust-overlay/issues/161
        stable-rust-native = pkgs.rust-bin.stable.${stable-rust-version};

        llvmPackages = pkgs.llvmPackages_18;

        customStdenv = import ./llvmStdenv.nix {inherit pkgs llvmPackages;};

        customClang = pkgs.callPackage ./customClang.nix {
          inherit pkgs;
          stdenv = customStdenv;
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain (stable-rust.default.override {
          targets = ["x86_64-unknown-linux-musl"];
        });

        src = pkgs.lib.cleanSourceWith {
          src = craneLib.path ./.;
          filter = path: type: (craneLib.filterCargoSources path type);
        };

        commonArgs = {
          inherit src;
          inherit (pkgs.pkgsMusl) stdenv;
          strictDeps = true;
          buildInputs = [
            (pkgs.pkgsMusl.openssl.override {static = true;})
            pkgs.pkgsMusl.cacert
          ];
          nativeBuildInputs = [pkgs.pkg-config];
          CARGO_BUILD_TARGET = "x86_64-unknown-linux-musl";
          CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static -C target-feature=+aes,+sse2";
        };

        # Additional target for external dependencies to simplify caching.
        cargoArtifacts = craneLib.buildDepsOnly commonArgs;

        hooks = import ./pre-commit-hooks.nix {inherit pkgs nightly-rust;};

        inherit (nix2container.packages.${system}.nix2container) buildImage;

        marketmaker = craneLib.buildPackage (commonArgs
          // {
            inherit cargoArtifacts;
          });
      in rec {
        _module.args.pkgs = import self.inputs.nixpkgs {
          inherit system;
          overlays = [(import rust-overlay)];
        };
        packages = rec {
          default = marketmaker;

          image = buildImage {
            name = "marketmaker";
            config = {
              Entrypoint = [(pkgs.lib.getExe' marketmaker "marketmaker")];
              Labels = {
                "org.opencontainers.image.description" = "A marketmaker.";
                "org.opencontainers.image.documentation" = "None";
                "org.opencontainers.image.licenses" = "None";
                "org.opencontainers.image.revision" = "${self.rev or self.dirtyRev or "dirty"}";
                "org.opencontainers.image.source" = "None";
                "org.opencontainers.image.title" = "None";
                "org.opencontainers.image.vendor" = "None";
              };
            };
          };
        };
        checks = {
          tests = craneLib.cargoNextest (commonArgs
            // {
              inherit cargoArtifacts;
              cargoNextestExtraArgs = "--all";
              partitions = 1;
              partitionType = "count";
            });
        };
        pre-commit.settings = {inherit hooks;};
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = let
            bazel = pkgs.writeShellScriptBin "bazel" ''
              unset TMPDIR TMP
              exec ${pkgs.bazelisk}/bin/bazelisk "$@"
            '';
          in [
            # Development tooling goes here.
            bazel
            stable-rust-native.default
            pkgs.pre-commit
            pkgs.awscli2
            pkgs.skopeo
            pkgs.dive
            pkgs.cosign
            pkgs.kubectl
            pkgs.kubernetes-helm
            pkgs.cilium-cli
            pkgs.vale
            pkgs.trivy
            pkgs.docker-client
            pkgs.kind
            pkgs.tektoncd-cli
            (pkgs.pulumi.withPackages (ps: [ps.pulumi-language-go]))
            pkgs.go
            pkgs.kustomize

            customClang
            pkgs.openssl
          ];
          shellHook = ''
            # Generate the .pre-commit-config.yaml symlink when entering the
            # development shell.
            ${config.pre-commit.installationScript}

            # The Bazel and Cargo builds in nix require a Clang toolchain.
            # TODO(aaronmondal): The Bazel build currently uses the
            #                    irreproducible host C++ toolchain. Provide
            #                    this toolchain via nix for bitwise identical
            #                    binaries across machines.

            export RUSTFLAGS="-C target-feature=+aes,+sse2"
            export CC=clang
          '';
        };
      };
    };
}
