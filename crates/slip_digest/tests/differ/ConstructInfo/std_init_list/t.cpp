#include "side.h"
// WP-035, digest-kinds §7 `ConstructInfo` `std_init_list`: the same constructor and argument; `{{1, 2}}` passes the inner list as the argument (no initializer-list initialisation).
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
struct L { L(std::initializer_list<int>); L(int, int); };
#ifdef B
L v{{1, 2}};
#else
L v{1, 2};
#endif
