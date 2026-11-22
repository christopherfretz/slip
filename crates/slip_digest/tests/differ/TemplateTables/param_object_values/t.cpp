#include "side.h"
// §9.5 TemplateTables `param_object_values`: the value of a template parameter
// object is its unit's whole stream (and its key, by content).
struct S { int a; };
template <S s> struct X {};
#ifdef B
X<S{2}> v;
#else
X<S{1}> v;
#endif
