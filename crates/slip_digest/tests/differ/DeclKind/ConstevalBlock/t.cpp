#include "side.h"
// std: c++26
// §5.6 DeclKind `ConstevalBlock`: against a `static_assert`, in a block. A
// namespace-scope `consteval` block is indeterminate: its closure's owner
// is the namespace (digest §4.3; indeterminate/_/consteval-block-at-namespace-scope).
#ifdef B
void f() { static_assert(true); }
#else
void f() { consteval {} }
#endif
