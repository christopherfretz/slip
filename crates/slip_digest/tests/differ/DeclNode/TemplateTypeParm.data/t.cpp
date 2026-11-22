#include "side.h"
// §5.3 DeclNode `TemplateTypeParm.data`: a pack.
#ifdef B
template <class... T> struct S;
#else
template <class T> struct S;
#endif
