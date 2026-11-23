#include "side.h"
// spec/digest.md §2.1 (WP-119 item 6): a shadow its class hides is no unit. Side b removes the hiding member: the shadow of Base::f() is introduced, its unit appears, and D changes.
struct Base { virtual void f(); void f(int); };
#ifdef B
struct D : Base { using Base::f; };
#else
struct D : Base { using Base::f; void f() override; };
#endif
