{
  description = "Arcuru's Library";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
      inputs.rust-analyzer-src.follows = "";
    };

    flake-utils.url = "github:numtide/flake-utils";

    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };
  };

  outputs = {
    self,
    nixpkgs,
    crane,
    fenix,
    flake-utils,
    advisory-db,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};

      inherit (pkgs) lib;

      craneLib = crane.mkLib pkgs;
      src = craneLib.cleanCargoSource ./.;

      # Common arguments can be set here to avoid repeating them later
      commonArgs = {
        inherit src;
        strictDeps = true;

        buildInputs =
          [
            # Add additional build inputs here
          ]
          ++ lib.optionals pkgs.stdenv.isDarwin [
            # Additional darwin specific inputs can be set here
            # NB: darwin is untested
            pkgs.libiconv
          ];
      };

      craneLibLLvmTools =
        craneLib.overrideToolchain
        (fenix.packages.${system}.complete.withComponents [
          "cargo"
          "llvm-tools"
          "rustc"
        ]);

      # Build *just* the cargo dependencies (of the entire workspace),
      # so we can reuse all of that work (e.g. via cachix) when running in CI
      # It is *highly* recommended to use something like cargo-hakari to avoid
      # cache misses when building individual top-level-crates
      cargoArtifacts = craneLib.buildDepsOnly (commonArgs
        // {
          pname = "arcuru-lib-deps";
        });

      individualCrateArgs =
        commonArgs
        // {
          inherit cargoArtifacts;
          inherit (craneLib.crateNameFromCargoToml {inherit src;}) version;
          # NB: we disable tests since we'll run them all via cargo-nextest
          doCheck = false;
        };

      fileSetForCrate = crate:
        lib.fileset.toSource {
          root = ./.;
          fileset = lib.fileset.unions [
            ./Cargo.toml
            ./Cargo.lock
            (craneLib.fileset.commonCargoSources crate)
          ];
        };

      # Build the top-level crates of the workspace as individual derivations.
      # This allows consumers to only depend on (and build) only what they need.
      # Though it is possible to build the entire workspace as a single derivation,
      # so this is left up to you on how to organize things
      #
      # Note that the cargo workspace must define `workspace.members` using wildcards,
      # otherwise, omitting a crate (like we do below) will result in errors since
      # cargo won't be able to find the sources for all members.
      percentiletracker = craneLib.buildPackage (individualCrateArgs
        // {
          pname = "percentiletracker";
          cargoExtraArgs = "-p percentiletracker";
          src = fileSetForCrate ./percentiletracker;
        });
    in {
      checks = {
        # Build the crates as part of `nix flake check` for convenience
        inherit percentiletracker;

        # Run clippy (and deny all warnings) on the workspace source,
        # again, reusing the dependency artifacts from above.
        #
        # Note that this is done as a separate derivation so that
        # we can block the CI if there are issues here, but not
        # prevent downstream consumers from building our crate by itself.
        arcuru-lib-clippy = craneLib.cargoClippy (commonArgs
          // {
            inherit cargoArtifacts;
            pname = "arcuru-lib-clippy";
            cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          });

        arcuru-lib-doc = craneLib.cargoDoc (commonArgs
          // {
            inherit cargoArtifacts;
            pname = "arcuru-lib-doc";
          });

        # Check formatting
        arcuru-lib-fmt = craneLib.cargoFmt {
          inherit src;
          pname = "arcuru-lib-fmt";
        };

        arcuru-lib-toml-fmt = craneLib.taploFmt {
          src = pkgs.lib.sources.sourceFilesBySuffices src [".toml"];
          pname = "arcuru-lib-toml-fmt";
          # taplo arguments can be further customized below as needed
          # taploExtraArgs = "--config ./taplo.toml";
        };

        # Audit dependencies
        arcuru-lib-audit = craneLib.cargoAudit {
          inherit src advisory-db;
          pname = "arcuru-lib-audit";
        };

        # Run tests with cargo-nextest
        # Consider setting `doCheck = false` on other crate derivations
        # if you do not want the tests to run twice
        arcuru-lib-nextest = craneLib.cargoNextest (commonArgs
          // {
            inherit cargoArtifacts;
            pname = "arcuru-lib-nextest";
            partitions = 1;
            partitionType = "count";
            cargoNextestPartitionsExtraArgs = "--no-tests=pass";
          });
      };

      packages =
        {
          inherit percentiletracker;
        }
        // lib.optionalAttrs (!pkgs.stdenv.isDarwin) {
          arcuru-lib-llvm-coverage = craneLibLLvmTools.cargoLlvmCov (commonArgs
            // {
              inherit cargoArtifacts;
              pname = "arcuru-lib-llvm-coverage";
            });
        };

      apps = {
        percentiletracker = flake-utils.lib.mkApp {
          drv = percentiletracker;
        };
      };

      devShells.default = craneLib.devShell {
        # Inherit inputs from checks.
        checks = self.checks.${system};
      };
    });
}
