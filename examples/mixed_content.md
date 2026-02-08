# Data Science Notebook: Customer Churn Analysis

## Abstract

This document presents a **comprehensive analysis** of customer churn patterns using *machine learning techniques*. We analyze data from 10,000 customers across 24 months.

## 1. Data Overview

### Dataset Statistics

| Feature | Type | Missing | Unique | Mean | Std Dev |
|---------|------|---------|--------|------|---------|
| customer_id | int | 0% | 10000 | - | - |
| tenure_months | int | 0% | 72 | 32.4 | 24.6 |
| monthly_charges | float | 0.2% | 6531 | 64.76 | 30.09 |
| total_charges | float | 0.8% | 9823 | 2283.30 | 2266.77 |
| churn | bool | 0% | 2 | 0.265 | 0.441 |

### Feature Categories

#### Demographic Features

- **Gender**: Male (50.2%), Female (49.8%)
- **Senior Citizen**: Yes (16.2%), No (83.8%)
- **Partner**: Yes (48.3%), No (51.7%)
- **Dependents**: Yes (29.9%), No (70.1%)

#### Service Features

1. Phone Service
2. Multiple Lines
3. Internet Service (DSL, Fiber Optic, None)
4. Online Security
5. Online Backup
6. Device Protection
7. Tech Support
8. Streaming TV
9. Streaming Movies

## 2. Exploratory Data Analysis

### Churn Distribution

The overall churn rate is **26.5%**, with significant variation across segments:

| Segment | Churn Rate | Count | Revenue Impact |
|---------|------------|-------|----------------|
| Month-to-month | 42.7% | 3875 | $2.1M |
| One year | 11.3% | 1473 | $0.4M |
| Two year | 2.8% | 1695 | $0.1M |

### Key Findings

- Customers with **fiber optic** internet have 2.3x higher churn
- `tenure < 12 months` accounts for 47% of all churns
- Customers without *tech support* churn at 41.7% vs 15.2%
- Monthly charges above $70 correlate with 38% churn rate

## 3. Model Development

### Feature Engineering

```python
import pandas as pd
import numpy as np
from sklearn.preprocessing import StandardScaler, LabelEncoder

def engineer_features(df: pd.DataFrame) -> pd.DataFrame:
    # Tenure buckets
    df['tenure_bucket'] = pd.cut(
        df['tenure_months'],
        bins=[0, 6, 12, 24, 48, 72],
        labels=['0-6', '7-12', '13-24', '25-48', '49-72']
    )
    
    # Charge ratio
    df['charge_ratio'] = df['monthly_charges'] / (df['total_charges'] + 1)
    
    # Service count
    service_cols = ['phone_service', 'internet_service', 'online_security',
                    'online_backup', 'device_protection', 'tech_support']
    df['service_count'] = df[service_cols].apply(
        lambda x: sum(1 for v in x if v not in ['No', 'No internet service']),
        axis=1
    )
    
    # Interaction features
    df['tenure_x_charges'] = df['tenure_months'] * df['monthly_charges']
    
    return df
```

### Model Comparison

| Model | Accuracy | Precision | Recall | F1 Score | AUC-ROC |
|-------|----------|-----------|--------|----------|---------|
| Logistic Regression | 80.2% | 65.3% | 54.1% | 0.592 | 0.843 |
| Random Forest | 79.5% | 63.8% | 48.7% | 0.552 | 0.831 |
| Gradient Boosting | 82.1% | 68.9% | 58.3% | 0.632 | 0.867 |
| XGBoost | 83.4% | 71.2% | 61.5% | 0.660 | 0.879 |
| Neural Network | 81.8% | 67.5% | 56.9% | 0.618 | 0.858 |

### Best Model: XGBoost

```python
from xgboost import XGBClassifier
from sklearn.model_selection import GridSearchCV

param_grid = {
    'max_depth': [3, 5, 7],
    'learning_rate': [0.01, 0.05, 0.1],
    'n_estimators': [100, 200, 500],
    'min_child_weight': [1, 3, 5],
    'subsample': [0.8, 0.9, 1.0],
    'colsample_bytree': [0.8, 0.9, 1.0],
}

model = XGBClassifier(
    objective='binary:logistic',
    eval_metric='auc',
    use_label_encoder=False,
    random_state=42
)

grid_search = GridSearchCV(
    model, param_grid,
    cv=5, scoring='roc_auc',
    n_jobs=-1, verbose=1
)
grid_search.fit(X_train, y_train)

print(f"Best AUC-ROC: {grid_search.best_score_:.4f}")
print(f"Best params: {grid_search.best_params_}")
```

## 4. Feature Importance

### Top 10 Features

1. **tenure_months** - 0.187 importance score
2. **monthly_charges** - 0.142 importance score
3. **total_charges** - 0.098 importance score
4. **contract_type** - 0.091 importance score
5. **internet_service** - 0.078 importance score
6. **tech_support** - 0.065 importance score
7. **online_security** - 0.058 importance score
8. **charge_ratio** - 0.052 importance score
9. **payment_method** - 0.047 importance score
10. **service_count** - 0.041 importance score

## 5. Recommendations

### Immediate Actions

- Target customers with `tenure < 6 months` for retention campaigns
- Offer **bundled tech support** to fiber optic customers
- Implement *early warning system* for high-risk accounts
- Create loyalty rewards for month-to-month contract holders

### Long-term Strategy

| Initiative | Expected Impact | Timeline | Cost |
|-----------|----------------|----------|------|
| Loyalty Program | -15% churn | Q1 2026 | $200K |
| Service Bundling | -8% churn | Q2 2026 | $50K |
| Proactive Support | -12% churn | Q1 2026 | $150K |
| Price Optimization | -5% churn | Q3 2026 | $30K |

## 6. Conclusion

The XGBoost model achieves **83.4% accuracy** with an AUC-ROC of **0.879**, providing reliable churn predictions. Key drivers are *tenure*, *monthly charges*, and *contract type*. Implementing the recommended retention strategies could reduce churn by an estimated **20-25%**, saving approximately $1.2M annually.

---

*Analysis completed on 2025-12-28. Data Science Team - Confidential.*
