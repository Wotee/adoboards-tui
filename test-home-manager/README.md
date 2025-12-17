# Home Manager Test Configuration

This directory contains test configurations for the adoboards Home Manager module on macOS.

## Setup

The test flake references the parent directory's adoboards flake.

**Important**: Update `system` in flake.nix if you're on an Intel Mac:
- Apple Silicon (M1/M2/M3): `aarch64-darwin` (default)
- Intel Mac: `x86_64-darwin`

## Test Configurations

### 1. test - Single Board
```bash
nix build .#homeConfigurations.test.activationPackage
```

### 2. test-multi - Multiple Boards
```bash
nix build .#homeConfigurations.test-multi.activationPackage
```

### 3. test-defaults - Default Values
```bash
nix build .#homeConfigurations.test-defaults.activationPackage
```

## Running Tests

### Build Only (Dry Run)
```bash
# Single board test
nix build .#homeConfigurations.test.activationPackage

# Check what would be activated
./result/activate
```

### Check Generated Config
After building, inspect the generated config:
```bash
# Find the config in the nix store
find result -name "default-config.toml" -exec cat {} \;
```

### Full Activation (Careful!)
This will actually modify your Home Manager state:
```bash
./result/activate
```

Then verify at:
```bash
cat ~/Library/Application\ Support/adoboards/default-config.toml
```

## Verify Config Location

The module should place config at:
```
~/Library/Application Support/adoboards/default-config.toml
```

Check with:
```bash
ls -la ~/Library/Application\ Support/adoboards/
```

## Testing Checklist

- [ ] Single board config builds
- [ ] Multi board config builds  
- [ ] Defaults config builds
- [ ] Generated TOML is valid
- [ ] Config placed in correct macOS location
- [ ] Multiple boards have separate `[[boards]]` sections
- [ ] Values match configuration exactly

## Cleanup

Remove test config after testing:
```bash
rm -rf ~/Library/Application\ Support/adoboards/default-config.toml
```

## Notes

- This does NOT require you to be using Home Manager currently
- Builds are isolated and won't affect your system unless you run `activate`
- You can inspect generated files in the `result/` symlink without activating
