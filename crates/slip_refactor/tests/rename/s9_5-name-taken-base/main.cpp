struct B { void g(); };
struct C : B { void f(); };
void h(C c) { c.f(); }
