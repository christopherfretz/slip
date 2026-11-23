#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): a defaulted constructor makes the base POD for layout, whose tail padding a derived class does not reuse (kiln_abi `layout/props.rs:252`).
#ifdef B
struct P { int i; char c; P() = default; };
#else
struct P { int i; char c; P() {} };
#endif
struct S : P { char d; };
