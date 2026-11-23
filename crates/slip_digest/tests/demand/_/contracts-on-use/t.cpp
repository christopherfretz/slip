// std: c++26
#include "side.h"
// digest-kinds §11.9 (WP-045 fix round, item 2): Kiln installs a
// specialisation's contracts and `0-has-contracts` when it instantiates
// the definition (kiln_sema/src/contracts.rs:681-687).
template <class T> inline int f(T x) pre(x > 0) { return x; }
extern template int f<int>(int);
#ifdef B
int use() { return f(1); }
#endif
