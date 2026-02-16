#!/bin/bash

# Validation script for new unicode, math, and code capabilities
# This script generates PDFs from example markdown files and validates them

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUTPUT_DIR="${SCRIPT_DIR}/output"
BINARY="${SCRIPT_DIR}/../target/release/pdfcli"

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "================================================"
echo "PDF Conversion Validation Script"
echo "Testing Unicode, Math, and Code Support"
echo "================================================"
echo ""

# Create output directory
mkdir -p "${OUTPUT_DIR}"

# Check if binary exists
if [ ! -f "${BINARY}" ]; then
    echo -e "${YELLOW}Binary not found. Building release version...${NC}"
    cd "${SCRIPT_DIR}/.."
    cargo build --release
    cd "${SCRIPT_DIR}"
fi

# Function to convert and validate
convert_and_validate() {
    local input_file=$1
    local output_file=$2
    local description=$3
    
    echo -e "${YELLOW}Testing: ${description}${NC}"
    echo "  Input:  ${input_file}"
    echo "  Output: ${output_file}"
    
    if [ ! -f "${input_file}" ]; then
        echo -e "${RED}  ✗ Input file not found${NC}"
        return 1
    fi
    
    # Convert markdown to PDF
    if "${BINARY}" md-to-pdf "${input_file}" "${output_file}" --font Helvetica --font-size 12; then
        # Check if output file was created
        if [ -f "${output_file}" ]; then
            local file_size=$(stat -f%z "${output_file}" 2>/dev/null || stat -c%s "${output_file}" 2>/dev/null)
            if [ "${file_size}" -gt 1000 ]; then
                echo -e "${GREEN}  ✓ Success (${file_size} bytes)${NC}"
                return 0
            else
                echo -e "${RED}  ✗ File too small (${file_size} bytes)${NC}"
                return 1
            fi
        else
            echo -e "${RED}  ✗ Output file not created${NC}"
            return 1
        fi
    else
        echo -e "${RED}  ✗ Conversion failed${NC}"
        return 1
    fi
}

# Test counter
TOTAL_TESTS=0
PASSED_TESTS=0

# Test 1: Unicode Showcase
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if convert_and_validate \
    "${SCRIPT_DIR}/unicode_showcase.md" \
    "${OUTPUT_DIR}/unicode_showcase.pdf" \
    "Unicode Showcase (Multiple Scripts)"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
fi
echo ""

# Test 2: Math Showcase
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if convert_and_validate \
    "${SCRIPT_DIR}/math_showcase.md" \
    "${OUTPUT_DIR}/math_showcase.pdf" \
    "Math Showcase (Inline & Block Math)"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
fi
echo ""

# Test 3: Code Showcase
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if convert_and_validate \
    "${SCRIPT_DIR}/code_showcase.md" \
    "${OUTPUT_DIR}/code_showcase.pdf" \
    "Code Showcase (Multiple Languages)"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
fi
echo ""

# Test 4: Comprehensive Test
TOTAL_TESTS=$((TOTAL_TESTS + 1))
if convert_and_validate \
    "${SCRIPT_DIR}/comprehensive_test.md" \
    "${OUTPUT_DIR}/comprehensive_test.pdf" \
    "Comprehensive Test (Unicode + Math + Code)"; then
    PASSED_TESTS=$((PASSED_TESTS + 1))
fi
echo ""

# Test 5: Existing complex examples
if [ -f "${SCRIPT_DIR}/math_and_formulas.md" ]; then
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    if convert_and_validate \
        "${SCRIPT_DIR}/math_and_formulas.md" \
        "${OUTPUT_DIR}/math_and_formulas.pdf" \
        "Existing Math and Formulas"; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
    fi
    echo ""
fi

if [ -f "${SCRIPT_DIR}/mixed_content.md" ]; then
    TOTAL_TESTS=$((TOTAL_TESTS + 1))
    if convert_and_validate \
        "${SCRIPT_DIR}/mixed_content.md" \
        "${OUTPUT_DIR}/mixed_content.pdf" \
        "Existing Mixed Content"; then
        PASSED_TESTS=$((PASSED_TESTS + 1))
    fi
    echo ""
fi

# Summary
echo "================================================"
echo "Validation Summary"
echo "================================================"
echo -e "Total Tests:  ${TOTAL_TESTS}"
echo -e "Passed:       ${GREEN}${PASSED_TESTS}${NC}"
echo -e "Failed:       ${RED}$((TOTAL_TESTS - PASSED_TESTS))${NC}"

if [ ${PASSED_TESTS} -eq ${TOTAL_TESTS} ]; then
    echo -e "\n${GREEN}All tests passed! ✓${NC}"
    echo ""
    echo "Generated PDFs are in: ${OUTPUT_DIR}"
    echo ""
    echo "You can view them with:"
    echo "  open ${OUTPUT_DIR}/*.pdf"
    exit 0
else
    echo -e "\n${RED}Some tests failed! ✗${NC}"
    exit 1
fi
