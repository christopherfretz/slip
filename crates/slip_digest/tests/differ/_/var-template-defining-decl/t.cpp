// std: c++17
#include "side.h"
// §2.3, WP-026: a variable template's pattern is read from its defining
//  declaration, not the first (`templ.rs` `template_data`): the
//  initialiser's form differs.
#ifdef B
template <class T> extern T v;
template <class T> T v{};
#else
template <class T> extern T v;
template <class T> T v = T();
#endif
