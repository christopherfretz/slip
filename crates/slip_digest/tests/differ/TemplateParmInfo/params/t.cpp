#include "side.h"
// §9.2 TemplateParmInfo `params`: a template template parameter's own head.
#ifdef B
template <template <class, class> class P> struct X;
#else
template <template <class> class P> struct X;
#endif
