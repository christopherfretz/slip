#include "side.h"
// §9.2 TemplateInfo `pattern`: the templated declaration, written whole in the
// template's unit (a variable template's type is not in its key).
#ifdef B
template <class T> T* v;
#else
template <class T> T v;
#endif
