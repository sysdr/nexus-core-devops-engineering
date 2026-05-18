package main

import "testing"

func TestFieldTypeFromString(t *testing.T) {
	if fieldTypeFromString("u64") != 0 {
		t.Fatal("u64")
	}
	if fieldTypeFromString("str") != 3 {
		t.Fatal("str")
	}
}
