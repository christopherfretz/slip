#include "side.h"
// digest §2.2 (WP-032, check 3): on side B the constant initialiser of `u`
// instantiates `t<int>` at once, whose body declares `g` and `ext` first,
// at block scope, and `g` again as a local class's friend; Kiln chains the
// namespace declarations after them. The units `g` and `ext` are
// classified, read and keyed from their namespace-scope declarations, and
// read over them alone, on both sides.
template <class T> constexpr int t() {
  void g(int);
  extern int ext;
  struct L { friend void g(int); };
  return 0;
}
#ifdef B
constexpr int u = t<int>();
#endif
void g(int) {}
int ext = 1;
