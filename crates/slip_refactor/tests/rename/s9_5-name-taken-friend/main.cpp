namespace n { struct S { friend void g(S); }; void f(S); }
void h() { n::S s; n::f(s); f(s); }
