#include "side.h"
// §5.3 DeclKind `ClassTemplatePartialSpecialization`: added.
template <class> struct S {};
#ifdef B
template <class T> struct S<T *> {};
#endif
