#include "side.h"
// §5.3 DeclKind `NonTypeTemplateParm`: kind changed: a constant parameter against a template one.
#ifdef B
template <int> struct S;
#else
template <template <class> class> struct S;
#endif
