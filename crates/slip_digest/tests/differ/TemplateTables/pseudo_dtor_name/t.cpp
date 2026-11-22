#include "side.h"
// §9.5 TemplateTables `pseudo_dtor_name`: the identifier after `~` that named no
// type at the definition.
#ifdef B
template <class T> void f(T* p) { p->~M(); }
#else
template <class T> void f(T* p) { p->~N(); }
#endif
