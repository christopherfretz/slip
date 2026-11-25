namespace a { struct { int m; } v = {1}; }
int u() { return a::v.m; }
