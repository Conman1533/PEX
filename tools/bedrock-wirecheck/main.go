// Command bedrock-wirecheck decodes every packet PEX sent the pumpkin-tester Bedrock bot with
// gophertunnel's own codec, and fails if any packet doesn't decode or leaves bytes unread.
//
// The bot shares PEX's codec, so it can't see a field that both sides write and read the same
// wrong way. gophertunnel tracks the vanilla protocol independently, so it can.
//
//	PUMPKIN_TESTER_BEDROCK_DUMP=/tmp/dump cargo run -p pumpkin-tester -- --bedrock-only
//	go run ./tools/bedrock-wirecheck /tmp/dump/*.hex
//
// Each dump line is "<packet id> <payload hex>", the payload being the packet after its header,
// as the bot received it.
package main

import (
	"bufio"
	"bytes"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"reflect"
	"sort"
	"strconv"
	"strings"

	"github.com/sandertv/gophertunnel/minecraft/protocol"
	"github.com/sandertv/gophertunnel/minecraft/protocol/packet"
)

// maxExamples is how many failures are printed for each packet type.
const maxExamples = 3

type result struct {
	name     string
	ok       int
	failures []string
}

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintln(os.Stderr, "usage: bedrock-wirecheck <dump file>...")
		os.Exit(2)
	}
	pool := packet.NewServerPool()
	results := map[uint32]*result{}
	total, failed := 0, 0

	for _, path := range os.Args[1:] {
		// The shield's runtime id comes from the ItemRegistry each connection gets; item stacks
		// decode differently for it.
		shieldID := int32(0)
		file, err := os.Open(path)
		if err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(2)
		}
		scanner := bufio.NewScanner(file)
		scanner.Buffer(make([]byte, 0, 1<<20), 64<<20)
		line := 0
		for scanner.Scan() {
			line++
			id, payload, err := parseLine(scanner.Text())
			if err != nil {
				fmt.Fprintf(os.Stderr, "%s:%d: %v\n", path, line, err)
				os.Exit(2)
			}
			total++
			res := results[id]
			if res == nil {
				res = &result{name: fmt.Sprintf("id %d (unknown to gophertunnel)", id)}
				results[id] = res
			}
			newPacket, known := pool[id]
			if !known {
				failed++
				res.failures = append(res.failures, fmt.Sprintf("%s:%d: no such clientbound packet", path, line))
				continue
			}
			pk := newPacket()
			res.name = reflect.TypeOf(pk).Elem().Name()
			if err := decode(pk, payload, shieldID); err != nil {
				failed++
				res.failures = append(res.failures, fmt.Sprintf("%s:%d: %v", path, line, err))
				continue
			}
			res.ok++
			if registry, ok := pk.(*packet.ItemRegistry); ok {
				for _, item := range registry.Items {
					if item.Name == "minecraft:shield" {
						shieldID = int32(item.RuntimeID)
					}
				}
			}
		}
		if err := scanner.Err(); err != nil {
			fmt.Fprintf(os.Stderr, "%s: %v\n", path, err)
			os.Exit(2)
		}
		_ = file.Close()
	}

	ids := make([]uint32, 0, len(results))
	for id := range results {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] })
	for _, id := range ids {
		res := results[id]
		status := "ok"
		if len(res.failures) > 0 {
			status = fmt.Sprintf("FAILED %d", len(res.failures))
		}
		fmt.Printf("%4d %-36s %5d decoded  %s\n", id, res.name, res.ok, status)
		for i, failure := range res.failures {
			if i == maxExamples {
				fmt.Printf("       ... %d more\n", len(res.failures)-maxExamples)
				break
			}
			fmt.Printf("       %s\n", failure)
		}
	}
	fmt.Printf("%d packets, %d failed\n", total, failed)
	if total == 0 || failed > 0 {
		os.Exit(1)
	}
}

func parseLine(text string) (uint32, []byte, error) {
	idText, payloadText, _ := strings.Cut(strings.TrimSpace(text), " ")
	id, err := strconv.ParseUint(idText, 10, 32)
	if err != nil {
		return 0, nil, fmt.Errorf("bad packet id %q", idText)
	}
	payload, err := hex.DecodeString(payloadText)
	if err != nil {
		return 0, nil, fmt.Errorf("bad payload hex: %v", err)
	}
	return uint32(id), payload, nil
}

// decode runs gophertunnel's Marshal on payload, as its connection does: the reader panics on
// bad data, and bytes left over mean a field the decoder didn't expect.
func decode(pk packet.Packet, payload []byte, shieldID int32) (err error) {
	defer func() {
		if recovered := recover(); recovered != nil {
			if recoveredErr, ok := recovered.(error); ok {
				err = recoveredErr
			} else {
				err = errors.New(fmt.Sprint(recovered))
			}
		}
	}()
	buf := bytes.NewBuffer(payload)
	pk.Marshal(protocol.NewReader(buf, shieldID, false))
	if buf.Len() != 0 {
		rest := buf.Bytes()
		if len(rest) > 32 {
			rest = rest[:32]
		}
		return fmt.Errorf("%d unread bytes: %x", buf.Len(), rest)
	}
	return nil
}
