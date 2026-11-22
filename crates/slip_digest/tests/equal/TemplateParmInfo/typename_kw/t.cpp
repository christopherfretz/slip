#include "side.h"
// §9.2 TemplateParmInfo `typename_kw`: `typename` or `class` is spelling.
#ifdef B
template <typename T, template <typename> typename P> struct X { T t; };
#else
template <class T, template <class> class P> struct X { T t; };
#endif
