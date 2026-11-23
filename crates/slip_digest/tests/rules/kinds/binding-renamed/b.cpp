struct P { int a; int b; };
P p{1, 2};
auto [u, y] = p;
int use() { return u + y; }
