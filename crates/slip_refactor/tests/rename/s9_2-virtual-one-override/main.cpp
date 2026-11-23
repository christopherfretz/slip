namespace n {
struct C { virtual void f(); };
struct D : C { void f() override; };
}
void h(n::C& c) { c.f(); }
