#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.aux-aligned`: an over-aligned type: Kiln sets the bit from the type's alignment (new_delete.rs:2435), whichever function is chosen.
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
struct alignas(64) C { int i; };
struct D { int i; };
C *pc;
D *pd;
#ifdef B
int v = (delete pd, 0);
#else
int v = (delete pc, 0);
#endif
