// WP-156 fix round 2 (M-1, M-2; negative control): a correct relocation
// with no operand Kiln does not keep is not refused.
// extern template of a function: arguments int, while the TU elsewhere has a decltype of int
namespace legacy {
int k = 0;
decltype(k) k2 = 1;
template <class T> void use(T) {}
extern template void use<int>(int);
template void use<long>(long);
}
