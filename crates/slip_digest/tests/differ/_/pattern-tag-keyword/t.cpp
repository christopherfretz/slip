#include "side.h"
// WP-124 item 4 (Kiln WP-3027 location-only table tag_keyword_locs): the fact is on the AST, the table holds only where it was written.
template<class T> void f() {
#ifdef B
  typename T::X *p = nullptr;
#else
  struct T::X *p = nullptr;
#endif
}
