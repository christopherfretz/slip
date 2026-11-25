// WP-156 fix round 2 (M-1, M-2; negative control): a correct relocation
// with no operand Kiln does not keep is not refused.
// an integral_constant-like template-id with a non-literal argument names a class; its explicit-specialisation member
namespace legacy {
template <bool V> struct BC { static const bool value = V; void f(); };
constexpr bool yes = true;
BC<yes> bc;
template <> void BC<true>::f() {}
}
