import json
import sys

def process_json(input_json):
    data = json.loads(input_json)
    
    if '@next' in data and 'index' in data['@next']:
        # Increment the index and pick the corresponding operation
        index = data['@next']['index'] + 1
    else:
        # Initialize index to 0
        index = 0
    
    # Ensure the index is within the bounds of the operations array
    if index < len(data['operations']):
        data['@next'] = {
            'index': index,
            'operation': data['operations'][index]
        }
        data['status'] = "apply_operation"
    else:
        # Handle the case where the index is out of bounds
        data['status'] = "successful"
    
    return json.dumps(data, indent=2)

if __name__ == "__main__":
    if len(sys.argv) != 2:
        print("Usage: python process_json.py '<json-string>'")
        sys.exit(1)
    
    input_json = sys.argv[1]
    try:
        output_json = process_json(input_json)
        print(':::begin-tedge:::')
        print(output_json)
        print(':::end-tedge:::')
    except json.JSONDecodeError:
        print("Invalid JSON input", file=sys.stderr)
        sys.exit(1)
