{
  description = "Open Labbook core module (Rust)";

  inputs = {
    logos-module-builder.url = "github:logos-co/logos-module-builder/0.3.0";

    # Runtime dependency, declared as a flake input so scaffold gets an
    # authoritative pin rather than falling back to BASECAMP_DEPENDENCIES
    # (basecamp skill, dep-resolution step 3).
    #
    # v0.3.3, NOT scaffold's default 1fde1566: that rev publishes no `lidl`
    # output, so module-builder 0.3.0 refuses it with "lists dependencies that
    # publish no LIDL". Confirmed by evaluating packages.aarch64-darwin on
    # both revs — only v0.3.3 exposes `lidl` / `delivery_module-lidl`.
    delivery_module.url = "github:logos-co/logos-delivery-module/1d995bda3128e13f21ed17e0f925f4c04d2bd117";

    # Mandatory: without this a second logos-module-builder node enters
    # flake.lock and silently wins under scaffold (docs/08 R8).
    delivery_module.inputs.logos-module-builder.follows = "logos-module-builder";

    # storage_module v2.1.3 — the version basecamp 0.3.0 actually ships (it is
    # what a running 0.3.0 instance on this machine has installed). v3.0.2
    # exists and both publish `lidl`, but R8's lesson is that a module built
    # against a newer logos-protocol can install cleanly and then have its
    # calls refused at load time. NOTE: the published "latest" API reference
    # documents 3.0.2 and does NOT match 2.1.3 — see docs/08 R11.
    storage_module.url = "github:logos-co/logos-storage-module/59b494e411b31797d029c0edb7e243198e13e771";
    storage_module.inputs.logos-module-builder.follows = "logos-module-builder";
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
    };
}
