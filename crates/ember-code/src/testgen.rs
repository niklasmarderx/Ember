//! Automatic Test Generation
//!
//! AI-powered test generation for multiple languages and testing frameworks.

use crate::analyzer::{CodeSymbol, FileAnalysis, Language, SymbolKind, Visibility};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Supported testing frameworks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestFramework {
    // Rust
    RustBuiltin,

    // Python
    Pytest,
    Unittest,

    // JavaScript/TypeScript
    Jest,
    Mocha,
    Vitest,

    // Go
    GoTest,

    // Java
    JUnit5,
    JUnit4,
    TestNG,
}

impl TestFramework {
    /// Get default framework for a language
    pub fn default_for_language(language: Language) -> Self {
        match language {
            Language::Rust => TestFramework::RustBuiltin,
            Language::Python => TestFramework::Pytest,
            Language::JavaScript => TestFramework::Jest,
            Language::TypeScript => TestFramework::Vitest,
            Language::Go => TestFramework::GoTest,
            Language::Java => TestFramework::JUnit5,
            Language::Unknown => TestFramework::RustBuiltin,
        }
    }

    /// Get test file extension
    pub fn test_file_suffix(&self) -> &str {
        match self {
            TestFramework::RustBuiltin => "",
            TestFramework::Pytest | TestFramework::Unittest => "_test",
            TestFramework::Jest | TestFramework::Mocha | TestFramework::Vitest => ".test",
            TestFramework::GoTest => "_test",
            TestFramework::JUnit5 | TestFramework::JUnit4 | TestFramework::TestNG => "Test",
        }
    }
}

/// Test type/category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestType {
    /// Basic unit test
    Unit,
    /// Edge case test
    EdgeCase,
    /// Error/exception handling test
    ErrorHandling,
    /// Property-based test
    Property,
    /// Boundary value test
    Boundary,
    /// Integration test
    Integration,
    /// Snapshot test
    Snapshot,
    /// Performance test
    Performance,
}

/// A generated test case
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTest {
    /// Test name
    pub name: String,
    /// Test type
    pub test_type: TestType,
    /// Test description
    pub description: String,
    /// The generated test code
    pub code: String,
    /// Setup code (if needed)
    pub setup: Option<String>,
    /// Teardown code (if needed)
    pub teardown: Option<String>,
    /// Expected assertion count
    pub assertion_count: u32,
    /// Test tags/categories
    pub tags: Vec<String>,
    /// Confidence score (0-100)
    pub confidence: u32,
}

/// A test suite containing multiple tests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedTestSuite {
    /// Suite name (usually the module/file name)
    pub name: String,
    /// Target file being tested
    pub target_file: PathBuf,
    /// Output test file path
    pub test_file: PathBuf,
    /// Testing framework used
    pub framework: TestFramework,
    /// Language
    pub language: Language,
    /// Import statements
    pub imports: Vec<String>,
    /// Test cases
    pub tests: Vec<GeneratedTest>,
    /// Shared setup code
    pub shared_setup: Option<String>,
    /// Shared teardown code
    pub shared_teardown: Option<String>,
    /// Complete generated file content
    pub full_content: String,
}

/// Configuration for test generation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestGenConfig {
    /// Testing framework to use
    pub framework: Option<TestFramework>,
    /// Generate edge case tests
    pub include_edge_cases: bool,
    /// Generate error handling tests
    pub include_error_tests: bool,
    /// Generate property-based tests
    pub include_property_tests: bool,
    /// Minimum function visibility to test
    pub min_visibility: Visibility,
    /// Maximum tests per function
    pub max_tests_per_function: usize,
    /// Include test descriptions
    pub include_descriptions: bool,
    /// Test output directory (None = same as source)
    pub output_dir: Option<PathBuf>,
}

impl Default for TestGenConfig {
    fn default() -> Self {
        Self {
            framework: None,
            include_edge_cases: true,
            include_error_tests: true,
            include_property_tests: false,
            min_visibility: Visibility::Public,
            max_tests_per_function: 5,
            include_descriptions: true,
            output_dir: None,
        }
    }
}

/// The test generator
pub struct TestGenerator {
    config: TestGenConfig,
}

impl TestGenerator {
    /// Create a new test generator
    pub fn new() -> Self {
        Self {
            config: TestGenConfig::default(),
        }
    }

    /// Create with custom config
    pub fn with_config(config: TestGenConfig) -> Self {
        Self { config }
    }

    /// Generate tests for a file
    pub fn generate_tests(&self, analysis: &FileAnalysis, content: &str) -> GeneratedTestSuite {
        let framework = self
            .config
            .framework
            .unwrap_or_else(|| TestFramework::default_for_language(analysis.language));

        let lines: Vec<&str> = content.lines().collect();
        let mut tests = Vec::new();

        // Generate tests for each testable symbol
        for symbol in &analysis.symbols {
            if self.should_test_symbol(symbol) {
                let symbol_tests =
                    self.generate_tests_for_symbol(symbol, &lines, analysis.language, framework);
                tests.extend(symbol_tests);
            }
        }

        // Generate imports
        let imports = self.generate_imports(&analysis.path, analysis.language, framework);

        // Generate shared setup/teardown
        let shared_setup = self.generate_shared_setup(analysis.language, framework);
        let shared_teardown = self.generate_shared_teardown(analysis.language, framework);

        // Calculate test file path
        let test_file = self.calculate_test_file_path(&analysis.path, analysis.language, framework);

        // Generate complete file content
        let full_content = self.generate_full_test_file(
            &analysis.path,
            analysis.language,
            framework,
            &imports,
            &tests,
            &shared_setup,
            &shared_teardown,
        );

        GeneratedTestSuite {
            name: analysis
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("tests")
                .to_string(),
            target_file: analysis.path.clone(),
            test_file,
            framework,
            language: analysis.language,
            imports,
            tests,
            shared_setup,
            shared_teardown,
            full_content,
        }
    }

    /// Check if a symbol should be tested
    fn should_test_symbol(&self, symbol: &CodeSymbol) -> bool {
        // Only test functions and methods
        if symbol.kind != SymbolKind::Function && symbol.kind != SymbolKind::Method {
            return false;
        }

        // Check visibility
        match (&symbol.visibility, &self.config.min_visibility) {
            (Visibility::Private, Visibility::Public) => false,
            (Visibility::Private, Visibility::Protected) => false,
            (Visibility::Protected, Visibility::Public) => false,
            _ => true,
        }
    }

    /// Generate tests for a single symbol
    fn generate_tests_for_symbol(
        &self,
        symbol: &CodeSymbol,
        lines: &[&str],
        language: Language,
        framework: TestFramework,
    ) -> Vec<GeneratedTest> {
        let mut tests = Vec::new();
        let func_code = self.get_function_code(lines, symbol);

        // Basic happy path test
        tests.push(self.generate_happy_path_test(symbol, &func_code, language, framework));

        // Edge case tests
        if self.config.include_edge_cases && tests.len() < self.config.max_tests_per_function {
            tests.extend(self.generate_edge_case_tests(symbol, &func_code, language, framework));
        }

        // Error handling tests
        if self.config.include_error_tests && tests.len() < self.config.max_tests_per_function {
            tests.extend(self.generate_error_tests(symbol, &func_code, language, framework));
        }

        // Property-based tests
        if self.config.include_property_tests && tests.len() < self.config.max_tests_per_function {
            if let Some(prop_test) = self.generate_property_test(symbol, language, framework) {
                tests.push(prop_test);
            }
        }

        // Limit tests
        tests.truncate(self.config.max_tests_per_function);

        tests
    }

    /// Generate happy path test
    fn generate_happy_path_test(
        &self,
        symbol: &CodeSymbol,
        func_code: &str,
        language: Language,
        framework: TestFramework,
    ) -> GeneratedTest {
        let test_name = format!("test_{}_basic", self.to_snake_case(&symbol.name));
        let description = format!("Test basic functionality of {}", symbol.name);

        let code = self.generate_test_code(
            &test_name,
            &symbol.name,
            language,
            framework,
            TestType::Unit,
            &self.infer_test_inputs(func_code, language),
            &self.infer_expected_output(func_code, language),
        );

        GeneratedTest {
            name: test_name,
            test_type: TestType::Unit,
            description,
            code,
            setup: None,
            teardown: None,
            assertion_count: 1,
            tags: vec!["unit".to_string()],
            confidence: 80,
        }
    }

    /// Generate edge case tests
    fn generate_edge_case_tests(
        &self,
        symbol: &CodeSymbol,
        func_code: &str,
        language: Language,
        framework: TestFramework,
    ) -> Vec<GeneratedTest> {
        let mut tests = Vec::new();

        // Detect parameter types and generate appropriate edge cases
        let edge_cases = self.detect_edge_cases(func_code, language);

        for (i, (case_name, inputs, expected)) in edge_cases.into_iter().enumerate() {
            let test_name = format!("test_{}_{}", self.to_snake_case(&symbol.name), case_name);
            let description = format!("Test {} with edge case: {}", symbol.name, case_name);

            let code = self.generate_test_code(
                &test_name,
                &symbol.name,
                language,
                framework,
                TestType::EdgeCase,
                &inputs,
                &expected,
            );

            tests.push(GeneratedTest {
                name: test_name,
                test_type: TestType::EdgeCase,
                description,
                code,
                setup: None,
                teardown: None,
                assertion_count: 1,
                tags: vec!["edge_case".to_string()],
                confidence: 70,
            });

            if i >= 2 {
                // Limit edge case tests
                break;
            }
        }

        tests
    }

    /// Generate error handling tests
    fn generate_error_tests(
        &self,
        symbol: &CodeSymbol,
        func_code: &str,
        language: Language,
        framework: TestFramework,
    ) -> Vec<GeneratedTest> {
        let mut tests = Vec::new();

        // Check if function can return errors
        let can_error = match language {
            Language::Rust => func_code.contains("Result<") || func_code.contains("Option<"),
            Language::Python => func_code.contains("raise ") || func_code.contains("except"),
            Language::JavaScript | Language::TypeScript => {
                func_code.contains("throw ") || func_code.contains("catch")
            }
            Language::Go => func_code.contains("error") || func_code.contains("err !="),
            Language::Java => func_code.contains("throws ") || func_code.contains("catch"),
            _ => false,
        };

        if !can_error {
            return tests;
        }

        let test_name = format!("test_{}_error_handling", self.to_snake_case(&symbol.name));
        let code = self.generate_error_test_code(&test_name, &symbol.name, language, framework);

        tests.push(GeneratedTest {
            name: test_name,
            test_type: TestType::ErrorHandling,
            description: format!("Test error handling in {}", symbol.name),
            code,
            setup: None,
            teardown: None,
            assertion_count: 1,
            tags: vec!["error".to_string()],
            confidence: 65,
        });

        tests
    }

    /// Generate property-based test
    fn generate_property_test(
        &self,
        symbol: &CodeSymbol,
        language: Language,
        framework: TestFramework,
    ) -> Option<GeneratedTest> {
        // Property-based tests are only supported for certain frameworks
        let supported = matches!(
            (language, framework),
            (Language::Rust, TestFramework::RustBuiltin)
                | (Language::Python, TestFramework::Pytest)
        );

        if !supported {
            return None;
        }

        let test_name = format!("test_{}_property", self.to_snake_case(&symbol.name));

        let code = match language {
            Language::Rust => format!(
                r#"#[cfg(test)]
mod proptest_tests {{
    use super::*;
    use proptest::prelude::*;

    proptest! {{
        #[test]
        fn {}(input in any::<i32>()) {{
            // Property: function should not panic with any valid input
            let _ = {}(input);
        }}
    }}
}}"#,
                test_name, symbol.name
            ),
            Language::Python => format!(
                r#"from hypothesis import given, strategies as st

@given(st.integers())
def {}(input_val):
    """Property: function should handle any integer input."""
    result = {}(input_val)
    # Add property assertions here
    assert result is not None
"#,
                test_name, symbol.name
            ),
            _ => return None,
        };

        Some(GeneratedTest {
            name: test_name,
            test_type: TestType::Property,
            description: format!("Property-based test for {}", symbol.name),
            code,
            setup: None,
            teardown: None,
            assertion_count: 1,
            tags: vec!["property".to_string(), "fuzz".to_string()],
            confidence: 60,
        })
    }

    /// Generate test code for a specific test
    fn generate_test_code(
        &self,
        test_name: &str,
        func_name: &str,
        language: Language,
        framework: TestFramework,
        _test_type: TestType,
        inputs: &str,
        expected: &str,
    ) -> String {
        match (language, framework) {
            (Language::Rust, TestFramework::RustBuiltin) => format!(
                r#"#[test]
fn {}() {{
    // Arrange
    let input = {};
    
    // Act
    let result = {}(input);
    
    // Assert
    assert_eq!(result, {});
}}"#,
                test_name, inputs, func_name, expected
            ),
            (Language::Python, TestFramework::Pytest) => format!(
                r#"def {}():
    """Test {}."""
    # Arrange
    input_val = {}
    
    # Act
    result = {}(input_val)
    
    # Assert
    assert result == {}"#,
                test_name, func_name, inputs, func_name, expected
            ),
            (Language::Python, TestFramework::Unittest) => format!(
                r#"def {}(self):
    """Test {}."""
    # Arrange
    input_val = {}
    
    # Act
    result = {}(input_val)
    
    # Assert
    self.assertEqual(result, {})"#,
                test_name, func_name, inputs, func_name, expected
            ),
            (Language::JavaScript, TestFramework::Jest)
            | (Language::TypeScript, TestFramework::Jest) => format!(
                r#"test('{}', () => {{
    // Arrange
    const input = {};
    
    // Act
    const result = {}(input);
    
    // Assert
    expect(result).toBe({});
}});"#,
                test_name.replace('_', " "),
                inputs,
                func_name,
                expected
            ),
            (Language::JavaScript, TestFramework::Vitest)
            | (Language::TypeScript, TestFramework::Vitest) => format!(
                r#"test('{}', () => {{
    // Arrange
    const input = {};
    
    // Act
    const result = {}(input);
    
    // Assert
    expect(result).toBe({});
}});"#,
                test_name.replace('_', " "),
                inputs,
                func_name,
                expected
            ),
            (Language::Go, TestFramework::GoTest) => format!(
                r#"func {}(t *testing.T) {{
    // Arrange
    input := {}
    expected := {}
    
    // Act
    result := {}(input)
    
    // Assert
    if result != expected {{
        t.Errorf("{} = %v, want %v", result, expected)
    }}
}}"#,
                self.to_pascal_case(test_name),
                inputs,
                expected,
                func_name,
                func_name
            ),
            (Language::Java, TestFramework::JUnit5) => format!(
                r#"@Test
void {}() {{
    // Arrange
    var input = {};
    var expected = {};
    
    // Act
    var result = {}(input);
    
    // Assert
    assertEquals(expected, result);
}}"#,
                test_name, inputs, expected, func_name
            ),
            _ => format!(
                "// TODO: Generate test for {} with {} framework",
                func_name,
                framework.test_file_suffix()
            ),
        }
    }

    /// Generate error test code
    fn generate_error_test_code(
        &self,
        test_name: &str,
        func_name: &str,
        language: Language,
        framework: TestFramework,
    ) -> String {
        match (language, framework) {
            (Language::Rust, TestFramework::RustBuiltin) => format!(
                r#"#[test]
fn {}() {{
    // Arrange - invalid input that should cause error
    let invalid_input = todo!("Add invalid input");
    
    // Act
    let result = {}(invalid_input);
    
    // Assert
    assert!(result.is_err());
}}"#,
                test_name, func_name
            ),
            (Language::Python, TestFramework::Pytest) => format!(
                r#"def {}():
    """Test error handling in {}."""
    import pytest
    
    # Arrange - invalid input that should raise exception
    invalid_input = None  # TODO: Add invalid input
    
    # Act & Assert
    with pytest.raises(Exception):
        {}(invalid_input)"#,
                test_name, func_name, func_name
            ),
            (Language::JavaScript, TestFramework::Jest)
            | (Language::TypeScript, TestFramework::Jest) => format!(
                r#"test('{} throws error for invalid input', () => {{
    // Arrange - invalid input
    const invalidInput = null;
    
    // Act & Assert
    expect(() => {}(invalidInput)).toThrow();
}});"#,
                func_name, func_name
            ),
            (Language::Go, TestFramework::GoTest) => format!(
                r#"func {}(t *testing.T) {{
    // Arrange - invalid input
    invalidInput := nil
    
    // Act
    _, err := {}(invalidInput)
    
    // Assert
    if err == nil {{
        t.Error("Expected error for invalid input, got nil")
    }}
}}"#,
                self.to_pascal_case(test_name),
                func_name
            ),
            _ => format!("// TODO: Generate error test for {}", func_name),
        }
    }

    /// Generate imports for test file
    fn generate_imports(
        &self,
        source_file: &PathBuf,
        language: Language,
        framework: TestFramework,
    ) -> Vec<String> {
        let module_name = source_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("module");

        match (language, framework) {
            (Language::Rust, TestFramework::RustBuiltin) => vec![format!("use super::*;")],
            (Language::Python, TestFramework::Pytest) => vec![
                format!("import pytest"),
                format!("from {} import *", module_name),
            ],
            (Language::Python, TestFramework::Unittest) => vec![
                format!("import unittest"),
                format!("from {} import *", module_name),
            ],
            (Language::JavaScript, TestFramework::Jest) => {
                vec![format!("const {{ ... }} = require('./{}');", module_name)]
            }
            (Language::TypeScript, TestFramework::Jest)
            | (Language::TypeScript, TestFramework::Vitest) => vec![
                format!("import {{ describe, test, expect }} from 'vitest';"),
                format!("import {{ ... }} from './{}';", module_name),
            ],
            (Language::Go, TestFramework::GoTest) => vec!["import \"testing\"".to_string()],
            (Language::Java, TestFramework::JUnit5) => vec![
                "import org.junit.jupiter.api.Test;".to_string(),
                "import static org.junit.jupiter.api.Assertions.*;".to_string(),
            ],
            _ => vec![],
        }
    }

    /// Generate shared setup code
    fn generate_shared_setup(
        &self,
        language: Language,
        framework: TestFramework,
    ) -> Option<String> {
        match (language, framework) {
            (Language::Python, TestFramework::Pytest) => Some(
                r#"@pytest.fixture
def setup():
    """Shared test setup."""
    # Add setup code here
    yield
    # Add teardown code here"#
                    .to_string(),
            ),
            (Language::JavaScript, TestFramework::Jest)
            | (Language::TypeScript, TestFramework::Jest) => Some(
                r#"beforeEach(() => {
    // Setup before each test
});

afterEach(() => {
    // Cleanup after each test
});"#
                    .to_string(),
            ),
            (Language::Java, TestFramework::JUnit5) => Some(
                r#"@BeforeEach
void setUp() {
    // Setup before each test
}

@AfterEach
void tearDown() {
    // Cleanup after each test
}"#
                .to_string(),
            ),
            _ => None,
        }
    }

    /// Generate shared teardown code
    fn generate_shared_teardown(
        &self,
        _language: Language,
        _framework: TestFramework,
    ) -> Option<String> {
        // Teardown is usually included in setup for most frameworks
        None
    }

    /// Calculate test file path
    fn calculate_test_file_path(
        &self,
        source_file: &PathBuf,
        language: Language,
        framework: TestFramework,
    ) -> PathBuf {
        let base_dir = self.config.output_dir.clone().unwrap_or_else(|| {
            source_file
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf()
        });

        let file_stem = source_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("test");

        let extension = source_file
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("rs");

        let suffix = framework.test_file_suffix();

        match language {
            Language::Rust => {
                // Rust tests go in the same file or tests/ directory
                base_dir.join(format!("{}_test.{}", file_stem, extension))
            }
            Language::Python => base_dir.join(format!("test_{}.{}", file_stem, extension)),
            Language::JavaScript | Language::TypeScript => {
                base_dir.join(format!("{}{}.{}", file_stem, suffix, extension))
            }
            Language::Go => base_dir.join(format!("{}_test.go", file_stem)),
            Language::Java => base_dir.join(format!("{}Test.java", file_stem)),
            Language::Unknown => base_dir.join(format!("{}_test.txt", file_stem)),
        }
    }

    /// Generate complete test file content
    fn generate_full_test_file(
        &self,
        source_file: &PathBuf,
        language: Language,
        framework: TestFramework,
        imports: &[String],
        tests: &[GeneratedTest],
        shared_setup: &Option<String>,
        _shared_teardown: &Option<String>,
    ) -> String {
        let mut content = String::new();

        // File header
        let module_name = source_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("module");

        content.push_str(&self.generate_file_header(language, module_name));
        content.push('\n');

        // Imports
        for import in imports {
            content.push_str(import);
            content.push('\n');
        }
        content.push('\n');

        // Shared setup
        if let Some(setup) = shared_setup {
            content.push_str(setup);
            content.push_str("\n\n");
        }

        // Test class/module wrapper (language specific)
        match (language, framework) {
            (Language::Rust, _) => {
                content.push_str("#[cfg(test)]\n");
                content.push_str("mod tests {\n");
                content.push_str("    use super::*;\n\n");

                for test in tests {
                    // Indent each line
                    for line in test.code.lines() {
                        content.push_str("    ");
                        content.push_str(line);
                        content.push('\n');
                    }
                    content.push('\n');
                }

                content.push_str("}\n");
            }
            (Language::Python, TestFramework::Unittest) => {
                content.push_str(&format!(
                    "class Test{}(unittest.TestCase):\n",
                    self.to_pascal_case(module_name)
                ));

                for test in tests {
                    for line in test.code.lines() {
                        content.push_str("    ");
                        content.push_str(line);
                        content.push('\n');
                    }
                    content.push('\n');
                }

                content.push_str("\nif __name__ == '__main__':\n");
                content.push_str("    unittest.main()\n");
            }
            (Language::JavaScript, TestFramework::Jest)
            | (Language::TypeScript, TestFramework::Jest)
            | (Language::TypeScript, TestFramework::Vitest) => {
                content.push_str(&format!("describe('{}', () => {{\n", module_name));

                for test in tests {
                    for line in test.code.lines() {
                        content.push_str("    ");
                        content.push_str(line);
                        content.push('\n');
                    }
                    content.push('\n');
                }

                content.push_str("});\n");
            }
            _ => {
                for test in tests {
                    content.push_str(&test.code);
                    content.push_str("\n\n");
                }
            }
        }

        content
    }

    /// Generate file header comment
    fn generate_file_header(&self, language: Language, module_name: &str) -> String {
        let comment = match language {
            Language::Rust => format!(
                "//! Tests for {}\n//!\n//! Auto-generated by Ember Code Intelligence\n",
                module_name
            ),
            Language::Python => format!(
                "\"\"\"Tests for {}.\n\nAuto-generated by Ember Code Intelligence.\n\"\"\"\n",
                module_name
            ),
            Language::JavaScript | Language::TypeScript => format!(
                "/**\n * Tests for {}\n *\n * Auto-generated by Ember Code Intelligence\n */\n",
                module_name
            ),
            Language::Go => format!(
                "// Tests for {}\n//\n// Auto-generated by Ember Code Intelligence\n",
                module_name
            ),
            Language::Java => format!(
                "/**\n * Tests for {}\n *\n * Auto-generated by Ember Code Intelligence\n */\n",
                module_name
            ),
            Language::Unknown => format!("// Tests for {}\n", module_name),
        };
        comment
    }

    /// Get function code from lines
    fn get_function_code(&self, lines: &[&str], symbol: &CodeSymbol) -> String {
        let start = (symbol.start_line.saturating_sub(1)) as usize;
        let end = (symbol.end_line as usize).min(lines.len());
        lines[start..end].join("\n")
    }

    /// Infer test inputs from function code
    fn infer_test_inputs(&self, func_code: &str, language: Language) -> String {
        // Simple heuristic - look for parameter types
        match language {
            Language::Rust => {
                if func_code.contains("&str") || func_code.contains("String") {
                    "\"test\".to_string()".to_string()
                } else if func_code.contains("i32") || func_code.contains("i64") {
                    "42".to_string()
                } else if func_code.contains("bool") {
                    "true".to_string()
                } else if func_code.contains("Vec<") {
                    "vec![]".to_string()
                } else {
                    "todo!(\"Add input\")".to_string()
                }
            }
            Language::Python => {
                if func_code.contains("str") {
                    "\"test\"".to_string()
                } else if func_code.contains("int") {
                    "42".to_string()
                } else if func_code.contains("bool") {
                    "True".to_string()
                } else if func_code.contains("list") {
                    "[]".to_string()
                } else {
                    "None  # TODO: Add input".to_string()
                }
            }
            Language::JavaScript | Language::TypeScript => {
                if func_code.contains("string") {
                    "\"test\"".to_string()
                } else if func_code.contains("number") {
                    "42".to_string()
                } else if func_code.contains("boolean") {
                    "true".to_string()
                } else if func_code.contains("Array") || func_code.contains("[]") {
                    "[]".to_string()
                } else {
                    "null // TODO: Add input".to_string()
                }
            }
            _ => "/* TODO: Add input */".to_string(),
        }
    }

    /// Infer expected output from function code
    fn infer_expected_output(&self, func_code: &str, language: Language) -> String {
        // Very simplified - in real implementation would analyze function logic
        match language {
            Language::Rust => {
                if func_code.contains("-> bool") {
                    "true".to_string()
                } else if func_code.contains("-> String") || func_code.contains("-> &str") {
                    "\"expected\".to_string()".to_string()
                } else if func_code.contains("-> i32") || func_code.contains("-> i64") {
                    "0".to_string()
                } else if func_code.contains("-> Vec<") {
                    "vec![]".to_string()
                } else if func_code.contains("-> Option<") {
                    "Some(todo!(\"expected value\"))".to_string()
                } else if func_code.contains("-> Result<") {
                    "Ok(todo!(\"expected value\"))".to_string()
                } else {
                    "todo!(\"Add expected value\")".to_string()
                }
            }
            Language::Python => "None  # TODO: Add expected value".to_string(),
            Language::JavaScript | Language::TypeScript => {
                "undefined // TODO: Add expected value".to_string()
            }
            _ => "/* TODO: Add expected value */".to_string(),
        }
    }

    /// Detect edge cases based on function code
    fn detect_edge_cases(
        &self,
        func_code: &str,
        language: Language,
    ) -> Vec<(String, String, String)> {
        let mut cases = Vec::new();

        // String edge cases
        if func_code.contains("str") || func_code.contains("String") {
            cases.push((
                "empty_string".to_string(),
                self.empty_string_value(language),
                self.infer_expected_output(func_code, language),
            ));
        }

        // Numeric edge cases
        if func_code.contains("i32")
            || func_code.contains("i64")
            || func_code.contains("int")
            || func_code.contains("number")
        {
            cases.push((
                "zero".to_string(),
                "0".to_string(),
                self.infer_expected_output(func_code, language),
            ));
            cases.push((
                "negative".to_string(),
                "-1".to_string(),
                self.infer_expected_output(func_code, language),
            ));
        }

        // Collection edge cases
        if func_code.contains("Vec<")
            || func_code.contains("list")
            || func_code.contains("Array")
            || func_code.contains("[]")
        {
            cases.push((
                "empty_collection".to_string(),
                self.empty_collection_value(language),
                self.infer_expected_output(func_code, language),
            ));
        }

        cases
    }

    /// Get empty string value for language
    fn empty_string_value(&self, language: Language) -> String {
        match language {
            Language::Rust => "\"\".to_string()".to_string(),
            Language::Python => "\"\"".to_string(),
            Language::JavaScript | Language::TypeScript => "\"\"".to_string(),
            Language::Go => "\"\"".to_string(),
            Language::Java => "\"\"".to_string(),
            _ => "\"\"".to_string(),
        }
    }

    /// Get empty collection value for language
    fn empty_collection_value(&self, language: Language) -> String {
        match language {
            Language::Rust => "vec![]".to_string(),
            Language::Python => "[]".to_string(),
            Language::JavaScript | Language::TypeScript => "[]".to_string(),
            Language::Go => "nil".to_string(),
            Language::Java => "new ArrayList<>()".to_string(),
            _ => "[]".to_string(),
        }
    }

    // Helper functions
    fn to_snake_case(&self, s: &str) -> String {
        let mut result = String::new();
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() && i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        }
        result
    }

    fn to_pascal_case(&self, s: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = true;
        for c in s.chars() {
            if c == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_uppercase().next().unwrap());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }
        result
    }
}

impl Default for TestGenerator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_framework_default() {
        assert_eq!(
            TestFramework::default_for_language(Language::Rust),
            TestFramework::RustBuiltin
        );
        assert_eq!(
            TestFramework::default_for_language(Language::Python),
            TestFramework::Pytest
        );
        assert_eq!(
            TestFramework::default_for_language(Language::TypeScript),
            TestFramework::Vitest
        );
    }

    #[test]
    fn test_case_conversion() {
        let gen = TestGenerator::new();
        assert_eq!(gen.to_snake_case("myFunction"), "my_function");
        assert_eq!(gen.to_pascal_case("my_function"), "MyFunction");
    }
}
