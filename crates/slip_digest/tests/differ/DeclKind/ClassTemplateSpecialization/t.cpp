#include "side.h"
// §5.3 DeclKind `ClassTemplateSpecialization`: added.
template <class> struct S {};
#ifdef B
template <> struct S<int> {};
#endif
