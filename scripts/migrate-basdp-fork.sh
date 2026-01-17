#!/bin/bash

BASDP_FORK_FOLDER="/c/Users/micha/Downloads/basdp-nih-plug"

script_dir="$(dirname "$(realpath "${BASH_SOURCE[0]}")")"
repo_root="$(realpath "$script_dir/..")"

echo "=== BASDP Fork Migration Script ==="
echo "Script directory: $script_dir"
echo "Repository root: $repo_root"
echo "Target fork folder: $BASDP_FORK_FOLDER"
echo ""

# if not exists BASDP_FORK_FOLDER, clone it
echo "Checking if BASDP fork exists..."
cd "$(dirname "$BASDP_FORK_FOLDER")"
echo "Changed to directory: $(pwd)"

if [ ! -d "basdp-nih-plug" ]; then
    echo "BASDP fork not found. Cloning repository..."
    git clone https://github.com/basdp/nih-plug basdp-nih-plug
    if [ $? -eq 0 ]; then
        echo "[SUCCESS] Successfully cloned BASDP fork"
    else
        echo "[ERROR] Failed to clone BASDP fork"
        exit 1
    fi
else
    echo "[OK] BASDP fork already exists at: $(pwd)/basdp-nih-plug"
fi

echo ""
echo "Starting folder migration..."

# List of folders to copy (and overwrite fully) to script_dir from BASDP_FORK_FOLDER
folders_to_copy=(
    "src"
    "xtask"
    "nih_plug_derive"
    "nih_plug_xtask"
    "cargo_nih_plug"
)

echo "Folders to migrate: ${folders_to_copy[*]}"
echo ""

for folder in "${folders_to_copy[@]}"; do
    echo "Processing folder: $folder"
    
    if [ -d "$BASDP_FORK_FOLDER/$folder" ]; then
        echo "  → Source exists: $BASDP_FORK_FOLDER/$folder"
        
        if [ -d "$repo_root/$folder" ]; then
            echo "  → Removing existing destination: $repo_root/$folder"
            rm -rf "$repo_root/$folder"
        fi
        
        echo "  → Copying $folder to repository..."
        cp -r "$BASDP_FORK_FOLDER/$folder" "$repo_root/$folder"
        
        if [ $? -eq 0 ]; then
            echo "  ✓ Successfully copied $folder"
        else
            echo "  ✗ Failed to copy $folder"
        fi
    else
        echo "  ⚠ Warning: Source folder does not exist: $BASDP_FORK_FOLDER/$folder"
    fi
    echo ""
done

echo "=== Migration Complete ==="
echo "All specified folders have been processed."
