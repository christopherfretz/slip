#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `packed` on a redeclaration after the definition is dropped (kiln_sema `after_def.rs:57`): neither the layout nor a unit changes.
struct S { char c; int i; };
#ifdef B
struct __attribute__((packed)) S;
#endif
