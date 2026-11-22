#include "side.h"
// digest §2.2 (WP-032, check 3): on side B the instantiation of `t<int>`
// declares `ext` first, at block scope, and Kiln chains the namespace
// declarations after it. The unit `ext` is classified, read and keyed from
// its first namespace-scope declaration on both sides: its specifiers are
// not the block-scope `extern`'s.
template <class T> int t() { extern int ext; return ext; }
#ifdef B
int u = t<int>();
#endif
extern int ext;
int ext = 1;
