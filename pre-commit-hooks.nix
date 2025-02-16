{
  pkgs,
  nightly-rust,
  ...
}: let
  excludes = [];
in {
  # Default hooks
  trailing-whitespace-fixer = {
    inherit excludes;
    enable = true;
    name = "trailing-whitespace";
    description = "Remove trailing whitespace";
    entry = "${pkgs.python312Packages.pre-commit-hooks}/bin/trailing-whitespace-fixer";
    types = ["text"];
  };
  end-of-file-fixer = {
    inherit excludes;
    enable = true;
    name = "end-of-file-fixer";
    description = "Remove trailing whitespace";
    entry = "${pkgs.python312Packages.pre-commit-hooks}/bin/end-of-file-fixer";
    types = ["text"];
  };
  fix-byte-order-marker = {
    inherit excludes;
    enable = true;
    name = "fix-byte-order-marker";
    entry = "${pkgs.python312Packages.pre-commit-hooks}/bin/fix-byte-order-marker";
  };
  mixed-line-ending = {
    inherit excludes;
    enable = true;
    name = "mixed-line-ending";
    entry = "${pkgs.python312Packages.pre-commit-hooks}/bin/mixed-line-ending";
    types = ["text"];
  };
  check-case-conflict = {
    inherit excludes;
    enable = true;
    name = "check-case-conflict";
    entry = "${pkgs.python312Packages.pre-commit-hooks}/bin/check-case-conflict";
    types = ["text"];
  };
  detect-private-key = {
    inherit excludes;
    enable = true;
    name = "detect-private-key";
    entry = "${pkgs.python312Packages.pre-commit-hooks}/bin/detect-private-key";
    types = ["text"];
  };
  forbid-binary-files = {
    excludes = [];
    enable = true;
    types = ["binary"];
    entry = let
      script = pkgs.writeShellScriptBin "forbid-binary-files" ''
        set -eu

        if [ $# -gt 0 ]; then
          for filename in "''${@}"; do
            printf "[\033[31mERROR\033[0m] Found binary file: ''${filename}"
          done
          exit 1
        fi
      '';
    in "${script}/bin/forbid-binary-files";
  };

  # Dockerfile
  hadolint.enable = true;

  # Nix
  alejandra.enable = true;
  statix.enable = true;
  deadnix.enable = true;

  # Rust
  rustfmt = {
    enable = true;
    packageOverrides.cargo = nightly-rust.cargo;
    packageOverrides.rustfmt = nightly-rust.rustfmt;
  };

  # Starlark
  bazel-buildifier-format = {
    enable = true;
    name = "buildifier format";
    description = "Format Starlark";
    entry = "${pkgs.bazel-buildtools}/bin/buildifier -lint=fix";
    types = ["bazel"];
  };
  bazel-buildifier-lint = {
    enable = true;
    name = "buildifier lint";
    description = "Lint Starlark";
    entry = "${pkgs.bazel-buildtools}/bin/buildifier -lint=warn";
    types = ["bazel"];
  };
}
