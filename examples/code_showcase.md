# Code Showcase

This document demonstrates code block support with syntax highlighting.

## Rust

```rust
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn main() {
    println!("Fibonacci(10) = {}", fibonacci(10));
}
```

## Python

```python
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[len(arr) // 2]
    left = [x for x in arr if x < pivot]
    middle = [x for x in arr if x == pivot]
    right = [x for x in arr if x > pivot]
    return quicksort(left) + middle + quicksort(right)

# Example usage
data = [3, 6, 8, 10, 1, 2, 1]
print(quicksort(data))
```

## JavaScript

```javascript
class BinaryTree {
    constructor(value) {
        this.value = value;
        this.left = null;
        this.right = null;
    }

    insert(value) {
        if (value < this.value) {
            if (this.left === null) {
                this.left = new BinaryTree(value);
            } else {
                this.left.insert(value);
            }
        } else {
            if (this.right === null) {
                this.right = new BinaryTree(value);
            } else {
                this.right.insert(value);
            }
        }
    }
}
```

## TypeScript

```typescript
interface User {
    id: number;
    name: string;
    email: string;
}

async function fetchUser(id: number): Promise<User> {
    const response = await fetch(`/api/users/${id}`);
    return await response.json();
}
```

## Go

```go
package main

import "fmt"

func isPrime(n int) bool {
    if n <= 1 {
        return false
    }
    for i := 2; i*i <= n; i++ {
        if n%i == 0 {
            return false
        }
    }
    return true
}

func main() {
    fmt.Println(isPrime(17))
}
```

## Java

```java
public class LinkedList<T> {
    private Node<T> head;
    
    private static class Node<T> {
        T data;
        Node<T> next;
        
        Node(T data) {
            this.data = data;
            this.next = null;
        }
    }
    
    public void add(T data) {
        Node<T> newNode = new Node<>(data);
        if (head == null) {
            head = newNode;
        } else {
            Node<T> current = head;
            while (current.next != null) {
                current = current.next;
            }
            current.next = newNode;
        }
    }
}
```

## SQL

```sql
SELECT u.name, COUNT(o.id) as order_count
FROM users u
LEFT JOIN orders o ON u.id = o.user_id
WHERE u.created_at >= '2024-01-01'
GROUP BY u.id, u.name
HAVING COUNT(o.id) > 5
ORDER BY order_count DESC
LIMIT 10;
```

## Bash

```bash
#!/bin/bash

# Backup script
BACKUP_DIR="/backup"
SOURCE_DIR="/data"
DATE=$(date +%Y%m%d)

tar -czf "${BACKUP_DIR}/backup_${DATE}.tar.gz" "${SOURCE_DIR}"

# Keep only last 7 days of backups
find "${BACKUP_DIR}" -name "backup_*.tar.gz" -mtime +7 -delete

echo "Backup completed: backup_${DATE}.tar.gz"
```

## Inline Code

Use `git commit -m "message"` to commit changes.

The `Array.prototype.map()` method creates a new array.

Install with `npm install package-name`.
