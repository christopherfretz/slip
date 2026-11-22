#include "side.h"
// WP-029: a template template parameter's own head is written in key form
// in a key (no default) and in unit form in a unit (its defaults), so a
// default in it changes the template's unit and not its key.
#ifdef B
template <template <class = int> class P> struct X;
#else
template <template <class> class P> struct X;
#endif
