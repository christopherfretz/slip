#include "side.h"
// §5.3 DeclKind `TemplateTemplateParm`: kind changed: a template parameter against a type one.
#ifdef B
template <template <class> class> struct S;
#else
template <class> struct S;
#endif
