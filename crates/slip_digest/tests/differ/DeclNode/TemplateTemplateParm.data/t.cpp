#include "side.h"
// §5.3 DeclNode `TemplateTemplateParm.data`: a pack (the head is in the key, §3.3).
#ifdef B
template <template <class> class... T> struct S;
#else
template <template <class> class T> struct S;
#endif
