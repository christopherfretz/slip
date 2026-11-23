#include "side.h"
// spec/digest.md §2.1 (WP-119 item 6): a using-declaration all of whose shadows are hidden still is a unit of its own (its key: its name and the scope it names), with no shadow unit; removing it (side b) is a difference, though the two classes behave alike.
struct Base { virtual void f(); };
#ifdef B
struct D : Base { void f() override; };
#else
struct D : Base { using Base::f; void f() override; };
#endif
