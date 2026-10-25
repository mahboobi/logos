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
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
    };
}
