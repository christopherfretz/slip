struct B { void g(int); };
struct D : B { using B::g; };
void use(D d) { d.g(1); }
