#include "side.h"
// §5.3 DeclNode `TemplateTemplateParm.data`: a pack.
#ifdef B
template <template <class> class... T> struct S;
#else
template <template <class> class T> struct S;
#endif
