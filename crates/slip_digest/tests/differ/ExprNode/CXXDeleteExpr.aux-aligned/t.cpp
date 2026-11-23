#include "side.h"
// WP-035, digest-kinds §7 `ExprNode` `CXXDeleteExpr.aux-aligned`: an over-aligned type: Kiln sets the bit from the type's alignment (new_delete.rs:2435), whichever function is chosen. (WP-124: the stand-in initializer_list defines no constructor in its template, whose mem-initializers would be indeterminate, kiln/pattern-mem-initializers.)
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
struct alignas(64) C { int i; };
struct D { int i; };
C *pc;
D *pd;
#ifdef B
int v = (delete pd, 0);
#else
int v = (delete pc, 0);
#endif
