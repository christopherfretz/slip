#include "side.h"
// WP-124 item 4 (Kiln WP-3027 location-only table virt_spec_locs): `override` changes no meaning of a well-formed program; the digest leaves it out in a class that is not a template too.
template<class T> struct D : T {
#ifdef B
  void f();
#else
  void f() override;
#endif
};
