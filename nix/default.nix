{
  stdenv,
  rustPlatform,
  lib,
  buildGoModule,
  go,
  makeWrapper,
  pkg-config,
  libX11,
  gtk3,
  glib,
  cairo,
  gdk-pixbuf,
  libayatana-appindicator,
  libXtst,
  wrapGAppsHook3,
  git,
  dbus,
  fontconfig,
}:
let
  # The workspace root is virtual (no [package]); the mousehop
  # binary crate lives in mousehop/ as a workspace member. Pull the
  # package name from there and the version from the workspace root,
  # which still owns [workspace.package].
  appCargoToml = fromTOML (builtins.readFile ../mousehop/Cargo.toml);
  rootCargoToml = fromTOML (builtins.readFile ../Cargo.toml);
  pname = appCargoToml.package.name;
  version = rootCargoToml.workspace.package.version;
  nativeLibraries = [ gtk3 glib cairo gdk-pixbuf fontconfig libayatana-appindicator ];
  frontend = buildGoModule {
    pname = "mousehop-mygo";
    inherit version;
    src = lib.cleanSource ../mousehop-mygo;
    vendorHash = "sha256-6E3OVpIj5ulbXO4Su17CJEYXw/gygb1cwH//RVyMUDY=";
    subPackages = [ "." ];
    ldflags = [ "-X main.version=${version}" ];
    nativeBuildInputs = [ makeWrapper ];
    buildInputs = lib.optionals stdenv.isLinux nativeLibraries;
    doCheck = false; # Native renderer checks run in the platform CI with fonts.
    postInstall = lib.optionalString stdenv.isLinux ''
      wrapProgram $out/bin/mygo \
        --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath nativeLibraries}
    '' + ''
      mv $out/bin/mygo $out/bin/mousehop-mygo
    '';
  };
in
assert lib.assertMsg (lib.versionAtLeast go.version "1.27.3") "MyGo requires Go 1.27.3+; refresh the nixpkgs input before building this branch";
rustPlatform.buildRustPackage {
  inherit pname;
  inherit version;

  nativeBuildInputs = [
    pkg-config
    makeWrapper
    git
  ] ++ lib.optionals stdenv.isLinux [ wrapGAppsHook3 ];

  buildInputs = lib.optionals stdenv.isLinux [
    libX11
    libXtst
    dbus
    fontconfig
    gtk3
  ];

  src = builtins.path {
    name = pname;
    path = lib.cleanSource ../.;
  };

  cargoLock.lockFile = ../Cargo.lock;

  # Set Environment Variables
  RUST_BACKTRACE = "full";

  postInstall = ''
    makeWrapper ${frontend}/bin/mousehop-mygo $out/bin/mousehop-mygo \
      --add-flags "--backend $out/bin/mousehop"
    install -Dm444 mousehop/*.desktop -t $out/share/applications
    install -Dm444 mousehop/assets/com.mousehop.Mousehop.svg -t $out/share/icons/hicolor/scalable/apps
    install -Dm444 mousehop-mygo/THIRD_PARTY_NOTICES.md -t $out/share/licenses/mousehop
  '';

  meta = with lib; {
    description = "Mousehop is a mouse and keyboard sharing software";
    longDescription = ''
      Mousehop is a mouse and keyboard sharing software similar to universal-control on Apple devices. It allows for using multiple pcs with a single set of mouse and keyboard. This is also known as a Software KVM switch.
      The primary target is Wayland on Linux but Windows and MacOS and Linux on Xorg have partial support as well (see below for more details).
    '';
    mainProgram = pname;
    platforms = platforms.all;
  };
}
