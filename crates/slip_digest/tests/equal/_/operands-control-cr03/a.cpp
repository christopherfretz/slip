// WP-156 fix round 3 (negative control, the third review's cr03):
// B1, G01: `sizeof(decltype(obj))` and a destructor call `p->~Foo()`.
// A correct relocation: no unit is indeterminate and every unit is equal.
namespace legacy {
struct Foo { ~Foo(); };
Foo::~Foo() {}
Foo obj;
unsigned long g() { return sizeof(decltype(obj)); }
void h(Foo* p) { p->~Foo(); }
}
