// WP-156 fix round 3 (negative control, the third review's cr04):
// B1: an alias template-id in a cast and an explicit instantiation elsewhere.
// A correct relocation: no unit is indeterminate and every unit is equal.
namespace acme {
template <class T> using id_t = T;
int k = 0;
long z = static_cast<id_t<long>>(k);
template <class T> void use(T) {}
template void use<long>(long);
}
