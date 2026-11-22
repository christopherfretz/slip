#include "side.h"
// WP-035, digest-kinds §7 `_` `init-list-constructor-added`: `T x{1}` chooses an added `initializer_list` constructor.
namespace std {
using size_t = decltype(sizeof 0);
enum class align_val_t : size_t {};
template <class E> class initializer_list {
  const E *b; size_t n;
  constexpr initializer_list(const E *b, size_t n) : b(b), n(n) {}
public:
  constexpr initializer_list() : b(nullptr), n(0) {}
  constexpr size_t size() const { return n; }
};
}
struct T { T(int);
#ifdef B
  T(std::initializer_list<int>);
#endif
};
T v{1};
