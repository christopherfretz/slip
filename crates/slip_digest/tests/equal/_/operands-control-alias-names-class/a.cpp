// WP-156 fix round 2 (M-1, M-2; negative control): a correct relocation
// with no operand Kiln does not keep is not refused.
// an alias template naming a class outright: that class's out-of-line members
namespace legacy {
struct Pl { void f(); };
template <class T> using AnyPl = Pl;
void Pl::f() {}
}
