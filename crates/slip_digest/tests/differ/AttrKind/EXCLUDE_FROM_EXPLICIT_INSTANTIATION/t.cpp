#include "side.h"
// §10.1 AttrKind: `EXCLUDE_FROM_EXPLICIT_INSTANTIATION`: the attribute on one side only.
#ifdef B
template <class T> struct X { __attribute__((exclude_from_explicit_instantiation)) void f(); };
#else
template <class T> struct X { void f(); };
#endif
