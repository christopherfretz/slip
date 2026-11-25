// WP-156 fix round 2 (M-1, M-2; negative control): a correct relocation
// with no operand Kiln does not keep is not refused.
// a non-dependent alias template specialisation somewhere makes its canonical type held
namespace acme {
template <class T> using id_t = T;
id_t<int> v = 0;
template <class T> T twice(T t) { return t + t; }
template int twice<int>(int);
}
