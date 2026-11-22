#include "side.h"
// §9.5 TemplateTables `specializations` (§11.8): a use adds an implicit
// instantiation to the template's list; the template's unit does not change.
template <class T> struct X { int a; };
#ifdef B
int h() { X<int> x; return x.a; }
#else

#endif
