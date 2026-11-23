#include "side.h"
// §5.3 DeclNode `TemplateTypeParm.data`: a pack (the head is in the key, §3.3).
#ifdef B
template <class... T> struct S;
#else
template <class T> struct S;
#endif
