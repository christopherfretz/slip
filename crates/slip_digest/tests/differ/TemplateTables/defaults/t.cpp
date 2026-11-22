#include "side.h"
// §9.5 TemplateTables `defaults`: a default template argument is in the
// template's unit (digest-kinds §2.3), not in its key.
#ifdef B
template <class T = long> struct X;
#else
template <class T = int> struct X;
#endif
