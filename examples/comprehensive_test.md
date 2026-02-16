# Comprehensive Test Document

This document combines unicode, math, and code to test all new capabilities.

## Section 1: Multilingual Content

### English
Hello World! This is a test of PDF generation with unicode support.

### 中文 (Chinese)
你好世界！这是PDF生成的测试。数学公式：$E = mc^2$

### 日本語 (Japanese)
こんにちは世界！数式：$\pi \approx 3.14159$

### 한국어 (Korean)
안녕하세요！수학: $a^2 + b^2 = c^2$

## Section 2: Mathematical Content

The area of a circle with radius $r$ is given by:

$$
A = \pi r^2
$$

The volume of a sphere is:

$$
V = \frac{4}{3}\pi r^3
$$

### Statistical Formulas

Mean: $\mu = \frac{1}{n}\sum_{i=1}^{n} x_i$

Standard deviation:

$$
\sigma = \sqrt{\frac{1}{n}\sum_{i=1}^{n}(x_i - \mu)^2}
$$

## Section 3: Code Examples

### Python with Unicode Comments

```python
# 计算斐波那契数列 (Calculate Fibonacci sequence)
def fibonacci(n):
    """
    Calcule la séquence de Fibonacci
    Вычисляет последовательность Фибоначчи
    """
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

# Test: 测试
print(f"Fibonacci(10) = {fibonacci(10)}")
```

### Rust with Math in Comments

```rust
// Calculate π using Leibniz formula: π = 4∑((-1)^n)/(2n+1)
fn calculate_pi(iterations: u32) -> f64 {
    let mut pi = 0.0;
    for n in 0..iterations {
        let sign = if n % 2 == 0 { 1.0 } else { -1.0 };
        pi += sign / (2.0 * n as f64 + 1.0);
    }
    4.0 * pi
}
```

## Section 4: Mixed Content Table

| Language | Greeting | Math Symbol |
|----------|----------|-------------|
| English  | Hello    | ∑ (sum)     |
| 中文     | 你好     | ∫ (integral)|
| 日本語   | こんにちは | ∞ (infinity)|
| 한국어   | 안녕하세요 | ≈ (approx)  |
| Русский  | Привет   | ∀ (forall)  |

## Section 5: Special Characters and Symbols

### Currency
$ € £ ¥ ₹ ₽ ₩ ₿

### Math Symbols
∀ ∃ ∈ ∉ ∑ ∏ ∫ ∂ ∇ √ ∞ ≠ ≈ ≤ ≥ ± × ÷

### Arrows
← → ↑ ↓ ↔ ⇐ ⇒ ⇔

### Greek Letters
α β γ δ ε ζ η θ ι κ λ μ ν ξ ο π ρ σ τ υ φ χ ψ ω

## Section 6: Complex Math with Code

The Fast Fourier Transform (FFT) algorithm:

$$
X_k = \sum_{n=0}^{N-1} x_n \cdot e^{-i2\pi kn/N}
$$

Implementation:

```python
import numpy as np

def fft(x):
    """
    Compute FFT using Cooley-Tukey algorithm
    Time complexity: O(N log N)
    """
    N = len(x)
    if N <= 1:
        return x
    
    even = fft(x[0::2])
    odd = fft(x[1::2])
    
    T = [np.exp(-2j * np.pi * k / N) * odd[k] for k in range(N // 2)]
    
    return [even[k] + T[k] for k in range(N // 2)] + \
           [even[k] - T[k] for k in range(N // 2)]
```

## Section 7: Unicode in Code Blocks

```javascript
// Unicode variable names (valid in modern JS)
const π = Math.PI;
const 圆周率 = π;
const радиус = 5;

// Calculate area: A = πr²
const площадь = π * радиус ** 2;

console.log(`Area (面积): ${площадь}`);
```

## Section 8: Escape Sequences Test

Special characters that need escaping:
- Backslash: \\
- Parentheses: \( \)
- Brackets: \[ \]
- Braces: \{ \}

Octal sequences: \101 = A, \102 = B, \103 = C

## Conclusion

This document tests:
✓ Unicode characters from multiple scripts
✓ Mathematical expressions (inline and block)
✓ Code blocks with syntax highlighting
✓ Special symbols and characters
✓ Mixed content in tables
✓ Escape sequences
