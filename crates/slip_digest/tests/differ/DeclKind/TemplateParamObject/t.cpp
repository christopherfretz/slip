// std: c++20
#include "side.h"
// §5.3 DeclKind `TemplateParamObject`: two specialisations on class-type constants that differ in one member (WP-039 item 3).
struct V { int a, b; };
template <V> struct N {};
#ifdef B
template <> struct N<V{1, 3}> {};
#else
template <> struct N<V{1, 2}> {};
#endif
