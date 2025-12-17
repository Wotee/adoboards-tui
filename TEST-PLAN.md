# Nix Configuration Test Plan

## Overview
Test plan for validating the Nix-based configuration management system for adoboards.

## Prerequisites
- Nix with flakes enabled
- Home Manager (for HM tests)
- nix-darwin (optional, for macOS tests)
- Clean test environment (no existing adoboards config)

---

## Test Suite 1: Flake Validation

### Test 1.1: Flake Check
**Objective**: Verify flake structure is valid
```bash
nix flake check
```
**Expected**: No errors, all outputs validate

### Test 1.2: Show Flake Outputs
**Objective**: Verify all expected outputs exist
```bash
nix flake show
```
**Expected**:
- `packages.{system}.default` exists
- `packages.{system}.config-generator` exists
- `devShells.{system}.default` exists
- `homeManagerModules.default` exists

### Test 1.3: Build Package
**Objective**: Verify the package builds successfully
```bash
nix build
```
**Expected**: Successfully builds, creates result symlink

---

## Test Suite 2: Config Generator Script

### Test 2.1: Generate with Defaults
**Objective**: Verify script generates config with default values
```bash
nix develop
adoboards-generate-config
```
**Expected**:
- Config file created at OS-specific location
- Contains `[[boards]]` section
- Has default values: `<organization>`, `<project>`, `<project> Team`

**Verify** (Linux):
```bash
cat ~/.config/adoboards/default-config.toml
```

**Verify** (macOS):
```bash
cat ~/Library/Application\ Support/adoboards/default-config.toml
```

### Test 2.2: Generate with Custom Values
**Objective**: Verify script accepts custom values
```bash
nix develop
adoboards-generate-config "testorg" "testproject" "Test Team"
```
**Expected**:
- Config file updated
- Contains custom values exactly as provided

### Test 2.3: Standalone Execution
**Objective**: Verify config-generator package works
```bash
nix run .#config-generator -- "myorg" "myproj" "My Team"
```
**Expected**: Config file created with provided values

### Test 2.4: OS Detection
**Objective**: Verify correct path on each OS
- **Linux**: Should use `~/.config/adoboards/`
- **macOS**: Should use `~/Library/Application Support/adoboards/`

---

## Test Suite 3: Home Manager Module

### Test 3.1: Module Import
**Objective**: Verify module can be imported
```nix
{
  imports = [ adoboards.homeManagerModules.default ];
}
```
**Expected**: No import errors

### Test 3.2: Enable with Defaults
**Objective**: Verify module with minimal config
```nix
programs.adoboards.enable = true;
```
**Build**:
```bash
home-manager switch --flake .#testuser
```
**Expected**:
- Config file created
- Contains one board with default values

### Test 3.3: Single Board Configuration
**Objective**: Verify single custom board
```nix
programs.adoboards = {
  enable = true;
  boards = [{
    organization = "testorg";
    project = "testproj";
    team = "Test Team";
  }];
};
```
**Expected**:
- Config file created with exactly one `[[boards]]` section
- Values match configuration

### Test 3.4: Multiple Boards Configuration
**Objective**: Verify multiple boards work
```nix
programs.adoboards = {
  enable = true;
  boards = [
    {
      organization = "org1";
      project = "proj1";
      team = "Team 1";
    }
    {
      organization = "org2";
      project = "proj2";
      team = "Team 2";
    }
  ];
};
```
**Expected**:
- Config file has two `[[boards]]` sections
- Each section has correct values
- Order is preserved

### Test 3.5: Partial Defaults
**Objective**: Verify default values are used when not specified
```nix
programs.adoboards = {
  enable = true;
  boards = [{
    organization = "myorg";
    # project and team should use defaults
  }];
};
```
**Expected**:
- Config has custom organization
- Project is `<project>`
- Team is `<project> Team`

### Test 3.6: Config File Location (Linux)
**Objective**: Verify correct path on Linux
```bash
ls -la ~/.config/adoboards/default-config.toml
```
**Expected**: File exists and is a symlink to nix store

### Test 3.7: Config File Location (macOS)
**Objective**: Verify correct path on macOS
```bash
ls -la ~/Library/Application\ Support/adoboards/default-config.toml
```
**Expected**: File exists and is a symlink to nix store

### Test 3.8: Idempotence
**Objective**: Verify rebuilding doesn't break config
```bash
home-manager switch --flake .#testuser
home-manager switch --flake .#testuser
```
**Expected**: 
- No errors on second build
- Config file unchanged

### Test 3.9: Configuration Updates
**Objective**: Verify config updates when options change
1. Build with one board
2. Verify config
3. Add another board to configuration
4. Rebuild
5. Verify config has both boards

**Expected**: Config reflects new configuration

---

## Test Suite 4: Development Shell

### Test 4.1: Enter Dev Shell
**Objective**: Verify dev shell activates
```bash
nix develop
```
**Expected**:
- Shell enters successfully
- Shows welcome message
- Shows helper command info

### Test 4.2: Shell Tools Available
**Objective**: Verify dev tools are available
```bash
nix develop -c bash -c "which cargo && which rustc && which rustfmt && which adoboards-generate-config"
```
**Expected**: All commands found

### Test 4.3: Shell Hook
**Objective**: Verify shellHook displays info
```bash
nix develop
```
**Expected**: Displays "Adoboards development shell" and command hint

---

## Test Suite 5: TOML Format Validation

### Test 5.1: Valid TOML Syntax
**Objective**: Verify generated config is valid TOML
```bash
nix develop
adoboards-generate-config "test" "test" "test"
# Use a TOML validator
python3 -c "import tomllib; open('~/.config/adoboards/default-config.toml')"
```
**Expected**: No TOML parsing errors

### Test 5.2: Special Characters
**Objective**: Verify handling of special characters
```bash
adoboards-generate-config "org-name" "project_name" "Team #1"
```
**Expected**: Config file has properly escaped/quoted values

### Test 5.3: Spaces in Values
**Objective**: Verify spaces are handled correctly
```bash
adoboards-generate-config "My Org" "My Project" "My Project Team"
```
**Expected**: Values preserved with spaces

---

## Test Suite 6: Integration Tests

### Test 6.1: Full Home Manager Setup
**Objective**: Complete end-to-end test
1. Create test flake with adoboards input
2. Configure Home Manager with adoboards module
3. Build Home Manager configuration
4. Verify config file exists
5. Run adoboards to verify it reads config

### Test 6.2: nix-darwin Setup
**Objective**: Verify works with nix-darwin (macOS only)
1. Add module to darwin configuration
2. Build darwin configuration
3. Verify config in macOS location

---

## Test Suite 7: Edge Cases

### Test 7.1: Empty Boards List
**Objective**: Handle empty boards gracefully
```nix
programs.adoboards = {
  enable = true;
  boards = [];
};
```
**Expected**: Config file created but empty (or with defaults)

### Test 7.2: Disable After Enable
**Objective**: Verify disabling removes config
1. Enable with config
2. Build
3. Disable (`enable = false`)
4. Rebuild

**Expected**: Config file removed

### Test 7.3: Config Directory Creation
**Objective**: Verify directory created if missing
```bash
rm -rf ~/.config/adoboards
adoboards-generate-config
```
**Expected**: Directory created automatically

---

## Success Criteria

✅ All flake outputs validate and build successfully  
✅ Config generator script works with defaults and custom values  
✅ Home Manager module creates config at correct OS-specific location  
✅ Multiple boards configuration works correctly  
✅ Generated TOML is valid and parseable  
✅ Dev shell provides all necessary tools  
✅ Configuration updates are reflected after rebuild  
✅ Special characters and spaces handled correctly  

---

## Test Execution Checklist

- [ ] Suite 1: Flake Validation (Tests 1.1-1.3)
- [ ] Suite 2: Config Generator Script (Tests 2.1-2.4)
- [ ] Suite 3: Home Manager Module (Tests 3.1-3.9)
- [ ] Suite 4: Development Shell (Tests 4.1-4.3)
- [ ] Suite 5: TOML Format Validation (Tests 5.1-5.3)
- [ ] Suite 6: Integration Tests (Tests 6.1-6.2)
- [ ] Suite 7: Edge Cases (Tests 7.1-7.3)

---

## Notes

- Run tests on both Linux and macOS if possible
- Clean test environment between suites to avoid state contamination
- Document any failures with exact error messages
- Keep test configurations in a separate test/ directory
