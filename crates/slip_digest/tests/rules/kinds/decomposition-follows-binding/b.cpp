struct P { int a; int b; };
P s{1, 2};
auto [z, y] = s;
int use() { return z + y; }
