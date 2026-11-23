#include "side.h"
// §5.3 DeclNode `ClassTemplateSpecialization.data`: the class's members.
template <class> struct S {};
#ifdef B
template <> struct S<int> { int a; };
#else
template <> struct S<int> {};
#endif
