// WP-156 fix round 2 (M-1, M-2; negative control): a correct relocation
// with no operand Kiln does not keep is not refused.
// an ordinary class named by a decltype of an expression somewhere; its out-of-line member
namespace acme {
struct Foo { void f(); int g() const; static int s; };
Foo obj;
decltype(obj) other;
void Foo::f() {}
int Foo::g() const { return 0; }
int Foo::s = 1;
}
