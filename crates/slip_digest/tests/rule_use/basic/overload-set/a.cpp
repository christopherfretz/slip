int f(int) { return 1; }
int f(long) { return 2; }
int f(char) { return 3; }
int use() { return f(1) + f(2L) + f('c'); }
