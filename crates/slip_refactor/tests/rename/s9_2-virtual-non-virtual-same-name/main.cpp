namespace n {
struct A { virtual void f(); };
struct X { void f(); };
}
void h(n::A& a, n::X& x) { a.f(); x.f(); }
