# Mathematical Foundations of Machine Learning

**Author**: Research Team | **Date**: February 2026

---

## 1. Linear Algebra Essentials

### 1.1 Vector Operations

The dot product of two vectors is defined as:

$$
\vec{a} \cdot \vec{b} = \sum_{i=1}^{n} a_i b_i
$$

The cross product in three dimensions:

$$
\vec{a} \times \vec{b} = \begin{vmatrix} \hat{i} & \hat{j} & \hat{k} \\ a_1 & a_2 & a_3 \\ b_1 & b_2 & b_3 \end{vmatrix}
$$

### 1.2 Matrix Decomposition

The Singular Value Decomposition (SVD) factorizes a matrix as:

$$
A = U \Sigma V^T
$$

where $U \in \mathbb{R}^{m \times m}$ is orthogonal, $\Sigma$ is diagonal, and $V \in \mathbb{R}^{n \times n}$ is orthogonal.

The eigenvalue equation:

$$
A \vec{v} = \lambda \vec{v}
$$

---

## 2. Calculus and Optimization

### 2.1 Gradient Descent

The gradient descent update rule:

$$
\theta_{t+1} = \theta_t - \alpha \nabla_\theta J(\theta_t)
$$

where $\alpha$ is the learning rate and $J(\theta)$ is the cost function.

### 2.2 Chain Rule for Backpropagation

$$
\frac{\partial L}{\partial w_{ij}} = \frac{\partial L}{\partial a_j} \cdot \frac{\partial a_j}{\partial z_j} \cdot \frac{\partial z_j}{\partial w_{ij}}
$$

### 2.3 Integration

The expected value of a continuous random variable:

$$
E[X] = \int_{-\infty}^{\infty} x \cdot f(x) \, dx
$$

The Gaussian integral:

$$
\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}
$$

---

## 3. Probability and Statistics

### 3.1 Bayes' Theorem

$$
P(A|B) = \frac{P(B|A) \cdot P(A)}{P(B)}
$$

### 3.2 Normal Distribution

The probability density function of the normal distribution:

$$
f(x) = \frac{1}{\sigma \sqrt{2\pi}} \exp\left(-\frac{(x - \mu)^2}{2\sigma^2}\right)
$$

### 3.3 Maximum Likelihood Estimation

The log-likelihood function:

$$
\ell(\theta) = \sum_{i=1}^{n} \log P(x_i | \theta)
$$

The MLE estimator maximizes this:

$$
\hat{\theta}_{MLE} = \arg\max_\theta \ell(\theta)
$$

---

## 4. Neural Network Formulations

### 4.1 Forward Pass

For a single neuron:

$z = \sum_{i=1}^{n} w_i x_i + b$

$a = \sigma(z)$

Common activation functions:

| Function | Formula | Range | Derivative |
|:---------|:--------|:------|:-----------|
| Sigmoid | 1/(1+e^-x) | (0, 1) | f(x)(1-f(x)) |
| Tanh | (e^x - e^-x)/(e^x + e^-x) | (-1, 1) | 1 - f(x)^2 |
| ReLU | max(0, x) | [0, inf) | 0 or 1 |
| Softmax | e^xi / sum(e^xj) | (0, 1) | Complex |

### 4.2 Loss Functions

Cross-entropy loss:

$$
L = -\sum_{i=1}^{C} y_i \log(\hat{y}_i)
$$

Mean squared error:

$$
L = \frac{1}{n} \sum_{i=1}^{n} (y_i - \hat{y}_i)^2
$$

### 4.3 Regularization

L2 regularization (Ridge):

$$
J_{reg}(\theta) = J(\theta) + \frac{\lambda}{2} \sum_{j=1}^{p} \theta_j^2
$$

L1 regularization (Lasso):

$$
J_{reg}(\theta) = J(\theta) + \lambda \sum_{j=1}^{p} |\theta_j|
$$

---

## 5. Information Theory

### 5.1 Entropy

Shannon entropy:

$$
H(X) = -\sum_{i=1}^{n} P(x_i) \log_2 P(x_i)
$$

### 5.2 KL Divergence

$$
D_{KL}(P \| Q) = \sum_{x} P(x) \log \frac{P(x)}{Q(x)}
$$

### 5.3 Mutual Information

$$
I(X; Y) = H(X) - H(X|Y) = H(Y) - H(Y|X)
$$

---

## 6. Advanced Topics

### 6.1 Attention Mechanism (Transformers)

The scaled dot-product attention:

$$
\text{Attention}(Q, K, V) = \text{softmax}\left(\frac{QK^T}{\sqrt{d_k}}\right) V
$$

### 6.2 Variational Inference (ELBO)

$$
\log P(x) \geq E_{q(z|x)}[\log P(x|z)] - D_{KL}(q(z|x) \| P(z))
$$

### 6.3 Fourier Transform

$$
\hat{f}(\xi) = \int_{-\infty}^{\infty} f(x) e^{-2\pi i x \xi} dx
$$

---

## Code Example: Gradient Descent in Rust

```rust
struct GradientDescent {
    learning_rate: f64,
    max_iterations: usize,
    tolerance: f64,
}

impl GradientDescent {
    fn minimize<F, G>(&self, f: F, grad: G, x0: &[f64]) -> Vec<f64>
    where
        F: Fn(&[f64]) -> f64,
        G: Fn(&[f64]) -> Vec<f64>,
    {
        let mut x = x0.to_vec();
        for _ in 0..self.max_iterations {
            let g = grad(&x);
            let norm: f64 = g.iter().map(|gi| gi * gi).sum::<f64>().sqrt();
            if norm < self.tolerance {
                break;
            }
            for (xi, gi) in x.iter_mut().zip(g.iter()) {
                *xi -= self.learning_rate * gi;
            }
        }
        x
    }
}
```

## Code Example: Matrix Operations in Python

```python
import numpy as np

def svd_compress(matrix, k):
    """Compress matrix using top-k singular values."""
    U, S, Vt = np.linalg.svd(matrix, full_matrices=False)
    U_k = U[:, :k]
    S_k = np.diag(S[:k])
    Vt_k = Vt[:k, :]
    return U_k @ S_k @ Vt_k

def gradient_descent(f, grad_f, x0, lr=0.01, epochs=1000):
    """Simple gradient descent optimizer."""
    x = np.array(x0, dtype=float)
    history = [f(x)]
    for _ in range(epochs):
        g = grad_f(x)
        x -= lr * g
        history.append(f(x))
    return x, history
```

---

## Summary Table

| Topic | Key Formula | Application |
|:------|:-----------|:------------|
| Gradient Descent | theta = theta - alpha * grad(J) | Optimization |
| Backpropagation | dL/dw = dL/da * da/dz * dz/dw | Training |
| Bayes' Theorem | P(A|B) = P(B|A)P(A)/P(B) | Classification |
| Cross-Entropy | L = -sum(y*log(y_hat)) | Loss function |
| Attention | softmax(QK^T/sqrt(d))V | Transformers |
| KL Divergence | sum(P*log(P/Q)) | Distribution comparison |
| Fourier Transform | integral(f*e^(-2pi*i*x*xi)) | Signal processing |

[^1]: All formulas assume standard mathematical notation conventions.
[^2]: Neural network formulations follow the notation from Goodfellow et al. (2016).
[^3]: Information theory measures use natural logarithm unless otherwise specified.

---

*Generated by pdf-rs v0.1.0 — Mathematical Foundations Series*
