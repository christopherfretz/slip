#include "side.h"
// spec/digest.md §2.1 (WP-119 item 6): a shadow its class hides is no unit. Side b changes the hiding member's signature: Base::f() is no longer hidden, its shadow's unit appears, and D changes. (The reverse, a shadow hidden by an edit: ColdDeclData/hidden_shadow.)
struct Base { void f(); void f(int); };
#ifdef B
struct D : Base { using Base::f; void f(long); };
#else
struct D : Base { using Base::f; void f(); };
#endif
