#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): `packed` on a declaration before the definition applies to the layout (kiln_abi `layout/props.rs:28`, every redeclaration).
#ifdef B
struct __attribute__((packed)) S;
#endif
struct S { char c; int i; };
