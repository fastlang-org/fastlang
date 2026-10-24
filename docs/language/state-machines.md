# State machines

A machine is a stateful scope with fields, labels, and handles for state transitions.
```rust
    machine Traffic -> {
        int32 count;

        @start -> {
            this.count = 0;
            goto @green;
        }

        @green -> {
            this.count += 1;
            goto @yellow;
        }

        @yellow -> {
            goto @red;
        }

        @red -> {
            goto @green;
        }
    }
```
Labels are states or executable transitions. Label nesting is not allowed. A label may declare an execution limit using the label-limit syntax; exceeding the limit produces a structured label-limit error.

Machines can define handles for call, leave, yield, and related transitions. The exact handle controls which transitions are legal and how a machine is resumed.

State-machine syntax is separate from ordinary fn control flow. A regular function should be used when persistent labels and transitions are not needed.

