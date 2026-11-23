#include "side.h"
// WP-124 item 4 (Kiln WP-3027 location-only table virt_spec_locs): the fact is on the AST, the table holds only where it was written.
template<class T> struct D : T {
#ifdef B
  void f() override;
#else
  void f() final;
#endif
};
