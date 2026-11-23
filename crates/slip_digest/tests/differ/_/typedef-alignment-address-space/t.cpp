#include "side.h"
// WP-107 (spec/landed/digest/WP-107.md): an address-space-qualified type of the typedef (Kiln's `ExtQual`).
typedef long L __attribute__((aligned(4)));
#ifdef B
__attribute__((address_space(1))) L v;
#else
__attribute__((address_space(1))) long v;
#endif
