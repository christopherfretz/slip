#include "side.h"
// WP-136 item 4 ("Its kin"): failed substitution of a constructor template
// (`function_ref`-style parameter SFINAE: `R::R<long>`).
template <class T> struct Trait {};
#ifdef B
template <> struct Trait<long> { using type = int; };
#else
template <> struct Trait<long> {};
#endif
struct R {
    template <class F> R(F, typename Trait<F>::type * = nullptr);
    R(int);
};
int u() {
    R r(1L);
    (void)r;
    return 0;
}
