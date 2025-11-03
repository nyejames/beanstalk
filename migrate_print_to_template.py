#!/usr/bin/env python3
"""
Migration script to convert print() calls to template syntax in Beanstalk test files.

Patterns:
- print("string") → ["string"]
- print(variable) → [variable]
- print([template]) → [template]
- print("string" + variable) → ["string", variable]
"""

import re
import os
import sys
from pathlib import Path
from typing import List, Tuple

class PrintMigrator:
    def __init__(self):
        self.migration_count = 0
        self.file_count = 0
        self.errors = []
        
    def migrate_line(self, line: str, line_num: int, filename: str) -> str:
        """Migrate a single line, handling all print() patterns."""
        original_line = line
        
        # Pattern 1: print([template]) → [template]
        # This must come first to avoid double-processing
        pattern1 = r'print\(\[(.*?)\]\)'
        if re.search(pattern1, line):
            line = re.sub(pattern1, r'[\1]', line)
            if line != original_line:
                self.migration_count += 1
                return line
        
        # Pattern 2: print("string") → ["string"]
        # Handle string literals with escaped quotes
        pattern2 = r'print\("((?:[^"\\]|\\.)*)"\)'
        if re.search(pattern2, line):
            line = re.sub(pattern2, r'["\1"]', line)
            if line != original_line:
                self.migration_count += 1
                return line
        
        # Pattern 3: print(variable) → [variable]
        # Match simple variable names (no spaces, operators, or function calls)
        pattern3 = r'print\(([a-z_][a-z0-9_]*)\)'
        if re.search(pattern3, line):
            line = re.sub(pattern3, r'[\1]', line)
            if line != original_line:
                self.migration_count += 1
                return line
        
        # Pattern 4: print(expression) → [expression]
        # For complex expressions like "string" + variable
        # This is a catch-all for remaining print() calls
        pattern4 = r'print\((.*?)\)(?=\s*(?:--|$))'
        if re.search(pattern4, line):
            # Extract the content
            match = re.search(pattern4, line)
            if match:
                content = match.group(1)
                # Check if it's a complex expression with operators
                if any(op in content for op in ['+', '-', '*', '/', '(', ')']):
                    # For now, wrap the entire expression in template
                    line = re.sub(pattern4, r'[\1]', line)
                    if line != original_line:
                        self.migration_count += 1
                        self.errors.append(f"{filename}:{line_num}: Complex expression may need manual review: {original_line.strip()}")
                        return line
        
        # If we still have print( that wasn't migrated, log it
        if 'print(' in line and line == original_line:
            self.errors.append(f"{filename}:{line_num}: Could not auto-migrate: {line.strip()}")
        
        return line
    
    def migrate_file(self, filepath: Path) -> Tuple[bool, str]:
        """Migrate a single file. Returns (changed, new_content)."""
        try:
            with open(filepath, 'r', encoding='utf-8') as f:
                lines = f.readlines()
            
            new_lines = []
            changed = False
            
            for i, line in enumerate(lines, 1):
                new_line = self.migrate_line(line, i, str(filepath))
                new_lines.append(new_line)
                if new_line != line:
                    changed = True
            
            return changed, ''.join(new_lines)
        
        except Exception as e:
            self.errors.append(f"{filepath}: Error reading file: {e}")
            return False, ""
    
    def migrate_directory(self, directory: Path, dry_run: bool = False) -> None:
        """Migrate all .bst files in a directory recursively."""
        if not directory.exists():
            print(f"Error: Directory {directory} does not exist")
            return
        
        bst_files = list(directory.rglob("*.bst"))
        
        if not bst_files:
            print(f"No .bst files found in {directory}")
            return
        
        print(f"Found {len(bst_files)} .bst files in {directory}")
        print(f"Mode: {'DRY RUN' if dry_run else 'LIVE MIGRATION'}")
        print("-" * 60)
        
        for filepath in bst_files:
            changed, new_content = self.migrate_file(filepath)
            
            if changed:
                self.file_count += 1
                print(f"✓ Migrated: {filepath}")
                
                if not dry_run:
                    with open(filepath, 'w', encoding='utf-8') as f:
                        f.write(new_content)
            else:
                # Check if file has print() calls that weren't migrated
                with open(filepath, 'r', encoding='utf-8') as f:
                    content = f.read()
                    if 'print(' in content:
                        print(f"⚠ Skipped (no changes): {filepath}")
    
    def print_report(self) -> None:
        """Print migration report."""
        print("\n" + "=" * 60)
        print("MIGRATION REPORT")
        print("=" * 60)
        print(f"Files modified: {self.file_count}")
        print(f"Print calls migrated: {self.migration_count}")
        
        if self.errors:
            print(f"\nWarnings/Errors: {len(self.errors)}")
            print("-" * 60)
            for error in self.errors:
                print(f"  {error}")
        else:
            print("\nNo errors or warnings!")
        
        print("=" * 60)

def main():
    import argparse
    
    parser = argparse.ArgumentParser(
        description="Migrate print() calls to template syntax in Beanstalk test files"
    )
    parser.add_argument(
        'directories',
        nargs='+',
        help='Directories to process (e.g., tests/cases/success tests/cases/failure)'
    )
    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Show what would be changed without modifying files'
    )
    
    args = parser.parse_args()
    
    migrator = PrintMigrator()
    
    for directory in args.directories:
        dir_path = Path(directory)
        print(f"\nProcessing directory: {dir_path}")
        migrator.migrate_directory(dir_path, dry_run=args.dry_run)
    
    migrator.print_report()
    
    # Exit with error code if there were errors
    sys.exit(1 if migrator.errors else 0)

if __name__ == "__main__":
    main()
