#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXNewExpr.bits-pass-align`: an over-aligned type passes its alignment. (WP-124: the stand-in initializer_list defines no constructor in its template, whose mem-initializers would be indeterminate, kiln/pattern-mem-initializers.)
namespace std {
using size_t = decltype(sizeof 0);
enum class align_val_t : size_t {};
template <class E> class initializer_list {
  const E *b = nullptr; size_t n = 0;
  constexpr initializer_list(const E *b, size_t n);
public:
  constexpr initializer_list() = default;
  constexpr size_t size() const { return n; }
};
}
struct alignas(64) Al { int i; };
struct Pl { int i; };
#ifdef B
void *v = new Pl;
#else
void *v = new Al;
#endif
