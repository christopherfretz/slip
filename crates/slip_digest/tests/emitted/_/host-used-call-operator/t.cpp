#include "side.h"
// WP-155 second fix round, rule 4: a mark too many, admitted. The call
// operator of a lambda in a default member initialiser has `used`; clang
// 19 emits nothing for it (the review's zz-rv155-k-hosts2 pre-2), but
// rule 1 marks the unit whose stream writes its definition in place.
template <class T> struct A { int x = []() __attribute__((used)) { return 1; }(); };
#ifndef B
int use() { A<int> a; return a.x; }
#endif
