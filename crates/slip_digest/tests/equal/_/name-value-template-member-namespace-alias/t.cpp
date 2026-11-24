#include "side.h"
// WP-125 item 3 (digest §9): a member function of a class template defined
// out of line, its class named through a namespace alias on side B; it
// consumes its __PRETTY_FUNCTION__'s length. The string prints the class's
// own scope, not the alias: "unsigned long ns::C<int>::m() [T = int]" on
// both sides (40 bytes, clang-19), so every unit is equal.
namespace ns { template <class T> struct C { unsigned long m(); }; }
namespace al = ns;
#ifdef B
template <class T> unsigned long al::C<T>::m() { return sizeof(__PRETTY_FUNCTION__); }
#else
template <class T> unsigned long ns::C<T>::m() { return sizeof(__PRETTY_FUNCTION__); }
#endif
unsigned long use() { return ns::C<int>{}.m(); }
