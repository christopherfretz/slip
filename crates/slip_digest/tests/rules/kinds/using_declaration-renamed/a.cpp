struct B { void f(int); };
struct D : B { using B::f; };
void use(D d) { d.f(1); }
