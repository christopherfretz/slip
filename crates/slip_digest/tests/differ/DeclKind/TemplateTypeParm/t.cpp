#include "side.h"
// §5.3 DeclKind `TemplateTypeParm`: kind changed: a type parameter against a constant one, in one template (the head is in the key, §3.3).
#ifdef B
template <class> struct S;
#else
template <int> struct S;
#endif
