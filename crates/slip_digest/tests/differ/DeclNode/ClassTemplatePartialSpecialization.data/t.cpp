#include "side.h"
// §5.3 DeclNode `ClassTemplatePartialSpecialization.data`: the class's members.
template <class> struct S {};
#ifdef B
template <class T> struct S<T *> { int a; };
#else
template <class T> struct S<T *> {};
#endif
